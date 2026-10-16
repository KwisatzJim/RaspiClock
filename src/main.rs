use chrono::Local;
use iced::widget::text;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Clone, serde::Deserialize)]
struct Config {
    latitude: f64,
    longitude: f64,
    timezone: String,
}

fn parse_config(contents: &str) -> Result<Config, String> {
    let config: Config =
        toml::from_str(contents).map_err(|e| format!("Invalid configuration: {e}"))?;
    if !config.latitude.is_finite() || !(-90.0..=90.0).contains(&config.latitude) {
        return Err("Latitude must be between -90 and 90".into());
    }
    if !config.longitude.is_finite() || !(-180.0..=180.0).contains(&config.longitude) {
        return Err("Longitude must be between -180 and 180".into());
    }
    // Validate against the system IANA zoneinfo database without changing process timezone.
    let zone = &config.timezone;
    if zone.is_empty()
        || zone
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || !zone
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/_+-".contains(c))
        || !PathBuf::from("/usr/share/zoneinfo").join(zone).is_file()
    {
        return Err(format!("Unknown IANA timezone: {zone}"));
    }
    Ok(config)
}

fn load_config() -> Result<Config, String> {
    let config_dir = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .ok_or("Neither XDG_CONFIG_HOME nor HOME is set")?;
    let path = config_dir.join("raspiclock/config.toml");
    let contents = std::fs::read_to_string(&path)
        .map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    parse_config(&contents)
}

#[derive(serde::Deserialize)]
struct WeatherResponse {
    current: CurrentWeather,
    daily: DailyWeather,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct CurrentWeather {
    temperature_2m: f32,
    weather_code: u8,
    is_day: u8,

    #[serde(skip)]
    sunrise: Option<String>,

    #[serde(skip)]
    sunset: Option<String>,
}

#[derive(serde::Deserialize)]
struct DailyWeather {
    sunrise: Vec<Option<String>>,
    sunset: Vec<Option<String>>,
}

struct RaspiClock {
    weather: Option<CurrentWeather>,
    weather_updated_at: Option<chrono::DateTime<chrono::Local>>,
    config: Option<Config>,
    config_error: Option<String>,
    weather_error: Option<String>,
    client: reqwest::Client,
    weather_in_flight: bool,
    weather_failures: u32,
    next_weather: Instant,
    system: SystemStatus,
    system_in_flight: bool,
    display_position: u8,
    window_width: f32,
    window_height: f32,
}

fn main() -> iced::Result {
    iced::application(boot, update, view)
        .title("RaspiClock")
        .subscription(subscription)
        .window(iced::window::Settings {
            decorations: false,
            ..Default::default()
        })
        .run()
}

fn boot() -> (RaspiClock, iced::Task<Message>) {
    let (config, config_error) = match load_config() {
        Ok(config) => (Some(config), None),
        Err(error) => {
            eprintln!("{error}");
            (None, Some(error))
        }
    };
    let weather = None;

    (
        RaspiClock {
            weather,
            weather_updated_at: None,
            config,
            config_error,
            weather_error: None,
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(15))
                .build()
                .expect("HTTP client initialization failed"),
            weather_in_flight: false,
            weather_failures: 0,
            next_weather: Instant::now(),
            system: SystemStatus::default(),
            system_in_flight: false,
            display_position: 0,
            window_width: 1280.0,
            window_height: 720.0,
        },
        iced::Task::batch([
            iced::Task::done(Message::RefreshWeather),
            iced::Task::done(Message::RefreshSystem),
        ]),
    )
}

#[derive(Debug, Clone)]
enum Message {
    Tick,
    RefreshWeather,
    ShiftDisplay,
    WindowResized(f32, f32),
    WeatherUpdated(Result<CurrentWeather, String>),
    RefreshSystem,
    SystemUpdated(SystemStatus),
}

fn update(state: &mut RaspiClock, message: Message) -> iced::Task<Message> {
    match message {
        Message::Tick => {
            if Instant::now() >= state.next_weather {
                return update(state, Message::RefreshWeather);
            }
        }
        Message::RefreshSystem => {
            if !state.system_in_flight {
                state.system_in_flight = true;
                return iced::Task::perform(
                    async {
                        tokio::task::spawn_blocking(read_system_status)
                            .await
                            .unwrap_or_default()
                    },
                    Message::SystemUpdated,
                );
            }
        }
        Message::SystemUpdated(status) => {
            state.system = status;
            state.system_in_flight = false;
        }

        Message::ShiftDisplay => {
            state.display_position = (state.display_position + 1) % 4;
        }

        Message::WindowResized(width, height) => {
            state.window_width = width;
            state.window_height = height;
        }

        Message::RefreshWeather => {
            if let Some(config) = &state.config
                && !state.weather_in_flight
            {
                let config = config.clone();
                let client = state.client.clone();
                state.weather_in_flight = true;

                return iced::Task::perform(
                    async move { fetch_weather(&client, &config).await },
                    Message::WeatherUpdated,
                );
            }
        }
        Message::WeatherUpdated(weather) => {
            state.weather_in_flight = false;
            match weather {
                Ok(weather) => {
                    state.weather = Some(weather);
                    state.weather_updated_at = Some(Local::now());
                    state.weather_error = None;
                    state.weather_failures = 0;
                    state.next_weather = Instant::now() + Duration::from_secs(15 * 60);
                }
                Err(error) => {
                    eprintln!("Weather update failed: {error}");
                    state.weather_error = Some(error);
                    state.weather_failures = state.weather_failures.saturating_add(1);
                    state.next_weather = Instant::now() + retry_delay(state.weather_failures);
                }
            }
        }
    }

    iced::Task::none()
}

fn view(state: &RaspiClock) -> iced::Element<'_, Message> {
    let now = Local::now();

    let width_scale = state.window_width / 1280.0;
    let height_scale = state.window_height / 720.0;
    let scale = width_scale.min(height_scale);

    let padding_small = 18.0 * scale;
    let padding_large = 22.0 * scale;

    let display_padding = match state.display_position {
        0 => iced::Padding {
            top: padding_small,
            right: padding_small,
            bottom: padding_large,
            left: padding_large,
        },
        1 => iced::Padding {
            top: padding_small,
            right: padding_large,
            bottom: padding_large,
            left: padding_small,
        },
        2 => iced::Padding {
            top: padding_large,
            right: padding_large,
            bottom: padding_small,
            left: padding_small,
        },
        _ => iced::Padding {
            top: padding_large,
            right: padding_small,
            bottom: padding_small,
            left: padding_large,
        },
    };

    let current_time = now.format("%-I:%M").to_string();
    let am_pm = now.format("%p").to_string();
    let current_day = now.format("%A").to_string();
    let current_date = now.format("%B %-d").to_string();

    let cpu_temp = state
        .system
        .cpu_temp
        .map(|temp| format!("{:.1}°C", temp))
        .unwrap_or_else(|| "--°C".to_string());

    let ethernet_connected = state
        .system
        .interface
        .as_deref()
        .is_some_and(|interface| interface.starts_with("eth") || interface.starts_with("en"));

    let network_status = match state.system.interface.as_deref() {
        Some(_) if ethernet_connected => "Connected",
        Some(interface) if interface.starts_with("wl") => "Wi-Fi • Connected",
        Some(_) => "Network • Connected",
        None => "Network • No default route",
    };

    let network_color = if state.system.interface.is_some() {
        iced::Color::from_rgb8(74, 222, 128)
    } else {
        iced::Color::from_rgb8(248, 113, 113)
    };

    let (weather_temp, weather_condition, weather_icon) = match &state.weather {
        Some(weather) => (
            format!("{:.0}°F", weather.temperature_2m),
            weather_description(weather.weather_code, weather.is_day).to_string(),
            weather_icon(weather.weather_code, weather.is_day),
        ),
        None => (
            "--°F".to_string(),
            "Weather unavailable".to_string(),
            "assets/weather/unknown.svg",
        ),
    };

    let weather_age = state
        .weather_updated_at
        .as_ref()
        .and_then(|updated| weather_age_text(updated, Local::now()));

    let (sunrise, sunset) = match &state.weather {
        Some(weather) => (
            weather
                .sunrise
                .as_deref()
                .and_then(format_sun_time)
                .unwrap_or_else(|| "--".to_string()),
            weather
                .sunset
                .as_deref()
                .and_then(format_sun_time)
                .unwrap_or_else(|| "--".to_string()),
        ),
        None => ("--".to_string(), "--".to_string()),
    };

    let time_row = iced::widget::row![
        text(current_time).size(170.0 * scale),
        text(am_pm).size(40.0 * scale),
    ]
    .spacing(12.0 * scale)
    .align_y(iced::Alignment::End);

    let clock_panel = iced::widget::column![
        time_row,
        text(current_day).size(64.0 * scale),
        text(current_date).size(56.0 * scale),
    ]
    .spacing(12.0 * scale)
    .align_x(iced::Alignment::Center);

    let sun_row = iced::widget::row![
        iced::widget::svg(icon_handle("assets/weather/sunrise.svg"))
            .width(48.0 * scale)
            .height(48.0 * scale),
        text(sunrise).size(38.0 * scale),
        iced::widget::svg(icon_handle("assets/weather/sunset.svg"))
            .width(48.0 * scale)
            .height(48.0 * scale),
        text(sunset).size(38.0 * scale),
    ]
    .spacing(10.0 * scale)
    .align_y(iced::Alignment::Center);

    let temperature_row = iced::widget::row![
        iced::widget::svg(icon_handle(weather_icon))
            .width(96.0 * scale)
            .height(96.0 * scale),
        text(weather_temp)
            .size(88.0 * scale)
            .color(iced::Color::from_rgb8(250, 204, 21)),
    ]
    .spacing(16.0 * scale)
    .align_y(iced::Alignment::Center);

    let service_status = if state.config_error.is_some() {
        "Configuration error — see log"
    } else if state.weather_error.is_some() {
        "Weather unavailable — retrying"
    } else if state.weather.is_none() {
        "Loading weather…"
    } else {
        ""
    };

    // Keep two lines available even when there is no warning, so updates never
    // move the temperature, condition, or sunrise/sunset rows.
    let weather_status = iced::widget::container(
        iced::widget::column![
            text(service_status)
                .size(22.0 * scale)
                .color(iced::Color::from_rgb8(251, 191, 36)),
            text(weather_age.unwrap_or_default())
                .size(22.0 * scale)
                .color(iced::Color::from_rgb8(148, 163, 184)),
        ]
        .spacing(6.0 * scale)
        .align_x(iced::Alignment::Center),
    )
    .height(80.0 * scale)
    .center_x(iced::Length::Fill);

    let info_panel = iced::widget::column![
        temperature_row,
        iced::widget::container(
            text(weather_condition)
                .size(56.0 * scale)
                .width(iced::Length::Fill)
                .align_x(iced::Alignment::Center)
                .color(iced::Color::from_rgb8(203, 213, 225)),
        )
        .center_y(148.0 * scale),
        sun_row,
        weather_status,
    ]
    .spacing(24.0 * scale)
    .align_x(iced::Alignment::Center);

    let main_panels = iced::widget::row![
        iced::widget::container(clock_panel)
            .width(iced::Length::FillPortion(3))
            .center_x(iced::Length::Fill)
            .center_y(iced::Length::Fill),
        iced::widget::container(info_panel)
            .width(iced::Length::FillPortion(2))
            .center_x(iced::Length::Fill)
            .center_y(iced::Length::Fill),
    ]
    .width(iced::Length::Fill)
    .height(iced::Length::Fill);

    let mut network_footer = iced::widget::Row::new()
        .spacing(12.0 * scale)
        .align_y(iced::Alignment::Center);
    if ethernet_connected {
        network_footer = network_footer.push(
            iced::widget::svg(icon_handle("assets/status/ethernet.svg"))
                .width(48.0 * scale)
                .height(48.0 * scale),
        );
    }
    let network_footer = network_footer.push(text(network_status).size(44.0 * scale).color(
        if state.system.interface.is_some() {
            iced::Color::from_rgb8(203, 213, 225)
        } else {
            network_color
        },
    ));

    let footer = iced::widget::row![
        iced::widget::svg(icon_handle("assets/status/pi.svg"))
            .width(48.0 * scale)
            .height(48.0 * scale),
        text(cpu_temp)
            .size(44.0 * scale)
            .color(iced::Color::from_rgb8(203, 213, 225)),
        text("•")
            .size(44.0 * scale)
            .color(iced::Color::from_rgb8(71, 85, 105)),
        network_footer,
    ]
    .spacing(14.0 * scale)
    .align_y(iced::Alignment::Center);

    iced::widget::mouse_area(
        iced::widget::container(
            iced::widget::column![
                main_panels,
                iced::widget::container(footer).center_x(iced::Length::Fill),
            ]
            .spacing(20.0 * scale)
            .width(iced::Length::Fill)
            .height(iced::Length::Fill),
        )
        .width(iced::Length::Fill)
        .height(iced::Length::Fill)
        .padding(display_padding)
        .style(|_| iced::widget::container::Style {
            background: Some(iced::Background::Color(iced::Color::from_rgb8(15, 23, 42))),
            text_color: Some(iced::Color::WHITE),
            ..Default::default()
        }),
    )
    .interaction(iced::mouse::Interaction::Hidden)
    .into()
}

fn subscription(_state: &RaspiClock) -> iced::Subscription<Message> {
    let clock = iced::time::every(Duration::from_secs(1)).map(|_| Message::Tick);

    let system = iced::time::every(Duration::from_secs(30)).map(|_| Message::RefreshSystem);

    let burn_in = iced::time::every(Duration::from_secs(5 * 60)).map(|_| Message::ShiftDisplay);

    let window_resize = iced::window::resize_events()
        .map(|(_id, size)| Message::WindowResized(size.width, size.height));

    iced::Subscription::batch([clock, system, burn_in, window_resize])
}

fn cpu_temperature() -> Option<f32> {
    let raw = std::fs::read_to_string("/sys/class/thermal/thermal_zone0/temp").ok()?;

    let millidegrees: f32 = raw.trim().parse().ok()?;

    Some(millidegrees / 1000.0)
}

#[derive(Debug, Clone, Default)]
struct SystemStatus {
    cpu_temp: Option<f32>,
    interface: Option<String>,
}

fn read_system_status() -> SystemStatus {
    SystemStatus {
        cpu_temp: cpu_temperature(),
        interface: active_network_interface(),
    }
}

fn route_interface(route: &str) -> Option<String> {
    route
        .lines()
        .filter(|line| line.starts_with("default "))
        .find_map(|line| {
            let parts: Vec<_> = line.split_whitespace().collect();
            parts
                .windows(2)
                .find(|pair| pair[0] == "dev")
                .map(|pair| pair[1].to_owned())
        })
}

fn active_network_interface() -> Option<String> {
    for family in ["-4", "-6"] {
        // The local route query runs on a blocking worker, never in the render path.
        if let Ok(output) = std::process::Command::new("ip")
            .args([family, "route", "show", "default"])
            .output()
            && output.status.success()
            && let Some(interface) = route_interface(&String::from_utf8_lossy(&output.stdout))
        {
            return Some(interface);
        }
    }
    None
}

fn retry_delay(failures: u32) -> Duration {
    Duration::from_secs((30u64 * 2u64.pow(failures.saturating_sub(1).min(5))).min(900))
}

fn decode_weather(response: WeatherResponse) -> CurrentWeather {
    let mut current = response.current;
    current.sunrise = response.daily.sunrise.into_iter().next().flatten();
    current.sunset = response.daily.sunset.into_iter().next().flatten();
    current
}

async fn fetch_weather(
    client: &reqwest::Client,
    config: &Config,
) -> Result<CurrentWeather, String> {
    let mut url = reqwest::Url::parse("https://api.open-meteo.com/v1/forecast")
        .expect("static weather URL is valid");
    url.query_pairs_mut().extend_pairs([
        ("latitude", config.latitude.to_string()),
        ("longitude", config.longitude.to_string()),
        ("current", "temperature_2m,weather_code,is_day".into()),
        ("daily", "sunrise,sunset".into()),
        ("temperature_unit", "fahrenheit".into()),
        ("timezone", config.timezone.clone()),
        ("forecast_days", "1".into()),
    ]);
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json::<WeatherResponse>()
        .await
        .map_err(|e| e.to_string())?;
    Ok(decode_weather(response))
}

fn weather_description(code: u8, is_day: u8) -> &'static str {
    match code {
        0 => {
            if is_day == 1 {
                "Sunny"
            } else {
                "Clear"
            }
        }
        1 => {
            if is_day == 1 {
                "Mostly Sunny"
            } else {
                "Mostly Clear"
            }
        }
        2 => "Partly Cloudy",
        3 => "Overcast",
        45 | 48 => "Fog",
        51 | 53 | 55 => "Drizzle",
        56 | 57 => "Freezing Drizzle",
        61 | 63 | 65 => "Rain",
        66 | 67 => "Freezing Rain",
        71 | 73 | 75 => "Snow",
        77 => "Snow Grains",
        80..=82 => "Rain Showers",
        85 | 86 => "Snow Showers",
        95 => "Thunderstorm",
        96 | 99 => "Thunderstorm + Hail",
        _ => "Unknown",
    }
}

fn weather_icon(code: u8, is_day: u8) -> &'static str {
    match code {
        0 => {
            if is_day == 1 {
                "assets/weather/clear.svg"
            } else {
                "assets/weather/clear-night.svg"
            }
        }
        1 => {
            if is_day == 1 {
                "assets/weather/mostly-clear.svg"
            } else {
                "assets/weather/mostly-clear-night.svg"
            }
        }
        2 => "assets/weather/partly-cloudy.svg",
        3 => "assets/weather/cloudy.svg",
        45 | 48 => "assets/weather/fog.svg",
        51 | 53 | 55 => "assets/weather/rain.svg",
        56 | 57 => "assets/weather/rain.svg",
        61 | 63 | 65 => "assets/weather/rain.svg",
        66 | 67 => "assets/weather/rain.svg",
        80..=82 => "assets/weather/rain.svg",
        71 | 73 | 75 => "assets/weather/snow.svg",
        77 => "assets/weather/snow.svg",
        85 | 86 => "assets/weather/snow.svg",
        95 | 96 | 99 => "assets/weather/thunderstorm.svg",
        _ => "assets/weather/unknown.svg",
    }
}

fn format_sun_time(value: &str) -> Option<String> {
    let datetime = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M").ok()?;

    Some(datetime.format("%-I:%M %p").to_string())
}

fn weather_age_text(
    updated_at: &chrono::DateTime<Local>,
    now: chrono::DateTime<Local>,
) -> Option<String> {
    let minutes = now.signed_duration_since(*updated_at).num_minutes().max(0);
    if minutes < 30 {
        None
    } else if minutes < 120 {
        Some(format!("Updated {minutes}m ago"))
    } else {
        Some(format!("Weather data {}h old", minutes / 60))
    }
}

fn icon_handle(path: &str) -> iced::widget::svg::Handle {
    let bytes: &'static [u8] = match path {
        "assets/status/pi.svg" => include_bytes!("../assets/status/pi.svg"),
        "assets/status/ethernet.svg" => include_bytes!("../assets/status/ethernet.svg"),
        "assets/weather/sunrise.svg" => include_bytes!("../assets/weather/sunrise.svg"),
        "assets/weather/sunset.svg" => include_bytes!("../assets/weather/sunset.svg"),
        "assets/weather/clear.svg" => include_bytes!("../assets/weather/clear.svg"),
        "assets/weather/clear-night.svg" => include_bytes!("../assets/weather/clear-night.svg"),
        "assets/weather/mostly-clear.svg" => include_bytes!("../assets/weather/mostly-clear.svg"),
        "assets/weather/mostly-clear-night.svg" => {
            include_bytes!("../assets/weather/mostly-clear-night.svg")
        }
        "assets/weather/partly-cloudy.svg" => include_bytes!("../assets/weather/partly-cloudy.svg"),
        "assets/weather/cloudy.svg" => include_bytes!("../assets/weather/cloudy.svg"),
        "assets/weather/fog.svg" => include_bytes!("../assets/weather/fog.svg"),
        "assets/weather/rain.svg" => include_bytes!("../assets/weather/rain.svg"),
        "assets/weather/snow.svg" => include_bytes!("../assets/weather/snow.svg"),
        "assets/weather/thunderstorm.svg" => include_bytes!("../assets/weather/thunderstorm.svg"),
        _ => include_bytes!("../assets/weather/unknown.svg"),
    };
    iced::widget::svg::Handle::from_memory(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weather_failure_retains_data_and_success_resets_retry() {
        let (mut state, _) = boot();
        state.weather_in_flight = true;
        let weather = CurrentWeather {
            temperature_2m: 72.0,
            weather_code: 0,
            is_day: 1,
            sunrise: None,
            sunset: None,
        };
        let _ = update(&mut state, Message::WeatherUpdated(Ok(weather.clone())));
        let updated_at = state.weather_updated_at;
        let _ = update(&mut state, Message::WeatherUpdated(Err("timeout".into())));
        assert_eq!(state.weather.as_ref().unwrap().temperature_2m, 72.0);
        assert_eq!(state.weather_updated_at, updated_at);
        assert!(!state.weather_in_flight);
        assert_eq!(state.weather_failures, 1);
        assert!(state.weather_error.is_some());
        let _ = update(&mut state, Message::WeatherUpdated(Ok(weather)));
        assert_eq!(state.weather_failures, 0);
        assert!(state.weather_error.is_none());
        assert!(state.next_weather > Instant::now() + Duration::from_secs(899));
    }

    #[test]
    fn polling_does_not_start_overlapping_work() {
        let (mut state, _) = boot();
        state.config =
            Some(parse_config("latitude = 0.0\nlongitude = 0.0\ntimezone = 'UTC'").unwrap());
        assert!(update(&mut state, Message::RefreshWeather).units() > 0);
        assert!(state.weather_in_flight);
        assert_eq!(update(&mut state, Message::RefreshWeather).units(), 0);
        assert!(update(&mut state, Message::RefreshSystem).units() > 0);
        assert_eq!(update(&mut state, Message::RefreshSystem).units(), 0);
        let _ = update(&mut state, Message::SystemUpdated(SystemStatus::default()));
        assert!(!state.system_in_flight);
    }

    #[test]
    fn validates_configuration() {
        let valid = "latitude = 30.27\nlongitude = -97.74\ntimezone = 'America/Chicago'";
        assert!(parse_config(valid).is_ok());
        for invalid in [
            valid.replace("30.27", "91"),
            valid.replace("-97.74", "-181"),
            valid.replace("America/Chicago", "../etc/passwd"),
            valid.replace("America/Chicago", "NoSuch/Zone"),
            valid.replace("30.27", "nan"),
            "invalid TOML".into(),
        ] {
            assert!(parse_config(&invalid).is_err());
        }
    }

    #[test]
    fn decodes_weather_and_handles_missing_sun_times() {
        let fixture = r#"current = { temperature_2m = 72.5, weather_code = 0, is_day = 1 }
            [daily]
            sunrise = ["2026-10-15T07:30"]
            sunset = ["2026-10-15T18:45"]"#;
        let weather = decode_weather(toml::from_str(fixture).unwrap());
        assert_eq!(weather.temperature_2m, 72.5);
        assert_eq!(weather.sunrise.as_deref(), Some("2026-10-15T07:30"));
        assert_eq!(
            format_sun_time(weather.sunset.as_deref().unwrap()).as_deref(),
            Some("6:45 PM")
        );
        let empty = fixture
            .replace("[\"2026-10-15T07:30\"]", "[]")
            .replace("[\"2026-10-15T18:45\"]", "[]");
        assert!(
            decode_weather(toml::from_str(&empty).unwrap())
                .sunrise
                .is_none()
        );
        assert!(format_sun_time("invalid").is_none());
    }

    #[test]
    fn decodes_polar_null_sun_times() {
        let response: WeatherResponse = serde_json::from_str(r#"{"current":{"temperature_2m":0,"weather_code":3,"is_day":0},"daily":{"sunrise":[null],"sunset":[null]}}"#).unwrap();
        let weather = decode_weather(response);
        assert!(weather.sunrise.is_none());
        assert!(weather.sunset.is_none());
    }

    #[test]
    fn stale_weather_boundaries_and_clock_correction() {
        let now = Local::now();
        for (minutes, expected) in [
            (-1, None),
            (29, None),
            (30, Some("Updated 30m ago")),
            (119, Some("Updated 119m ago")),
            (120, Some("Weather data 2h old")),
        ] {
            assert_eq!(
                weather_age_text(&(now - chrono::Duration::minutes(minutes)), now).as_deref(),
                expected
            );
        }
    }

    #[test]
    fn retries_are_bounded() {
        assert_eq!(retry_delay(1).as_secs(), 30);
        assert_eq!(retry_delay(2).as_secs(), 60);
        assert_eq!(retry_delay(u32::MAX).as_secs(), 900);
    }

    #[test]
    fn route_parsing_supports_both_families() {
        assert_eq!(
            route_interface("default via 192.0.2.1 dev eth0 proto dhcp").as_deref(),
            Some("eth0")
        );
        assert_eq!(
            route_interface("default via fe80::1 dev wlan0 proto ra").as_deref(),
            Some("wlan0")
        );
        assert!(route_interface("192.0.2.0/24 dev eth0").is_none());
    }

    #[test]
    fn icons_cover_unknown_and_night_conditions() {
        assert_eq!(weather_description(255, 1), "Unknown");
        assert_eq!(weather_icon(255, 1), "assets/weather/unknown.svg");
        assert_eq!(weather_description(0, 0), "Clear");
        assert_eq!(weather_icon(0, 0), "assets/weather/clear-night.svg");
        // Embedded handles work independently of the working directory.
        let _ = icon_handle(weather_icon(255, 1));
    }
}
