use iced::widget::text;
use chrono::Local;
use std::time::Duration;

#[derive(serde::Deserialize)]
struct Config {
    latitude: f64,
    longitude: f64,
    timezone: String,
}

fn load_config() -> Option<Config> {
    let config_dir = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|home| format!("{home}/.config"))
        })?;

    let config_path = format!("{config_dir}/raspiclock/config.toml");

    let contents = std::fs::read_to_string(config_path).ok()?;
    toml::from_str(&contents).ok()
}

#[derive(serde::Deserialize)]
struct WeatherResponse {
    current: CurrentWeather,
    daily: DailyWeather,
}

#[derive(serde::Deserialize)]
struct CurrentWeather {
    temperature_2m: f32,
    weather_code: u8,

    #[serde(skip)]
    sunrise: Option<String>,

    #[serde(skip)]
    sunset: Option<String>,
}

#[derive(serde::Deserialize)]
struct DailyWeather {
    sunrise: Vec<String>,
    sunset: Vec<String>,
}

struct RaspiClock {
    weather: Option<CurrentWeather>,
    config: Option<Config>,
    display_position: u8,
}

fn main() -> iced::Result {
    iced::application(boot, update, view)
        .title("RaspiClock")
        .subscription(subscription)
        .run()
}

fn boot() -> RaspiClock {
    let config = load_config();
    let weather = config.as_ref().and_then(fetch_weather);

    RaspiClock {
        weather,
        config,
        display_position: 0,
    }
}

#[derive(Debug, Clone)]
enum Message {
    Tick,
    RefreshWeather,
    ShiftDisplay,
}

fn update(state: &mut RaspiClock, message: Message) {
    match message {
        Message::Tick => {}

        Message::ShiftDisplay => {
            state.display_position = (state.display_position +1) % 4;
        }

        Message::RefreshWeather => {
            if let Some(config) = &state.config {
                if let Some(weather) = fetch_weather(config) {
                   state.weather = Some(weather);
                }
            }
        }
    }
}

fn view(_state: &RaspiClock) -> iced::Element<'_, Message> {
    let now = Local::now();

    let display_padding = match _state.display_position {
        0 => iced::Padding {
            top: 18.0,
            right: 18.0,
            bottom: 22.0,
            left: 22.0,
        },
        1 => iced::Padding {
            top: 18.0,
            right: 22.0,
            bottom: 22.0,
            left: 18.0,
        },
        2 => iced::Padding {
            top: 22.0,
            right: 22.0,
            bottom: 18.0,
            left: 18.0,
        },
        _ => iced::Padding {
            top: 22.0,
            right: 18.0,
            bottom: 18.0,
            left: 22.0,
        },
    };

    let current_time = now.format("%-I:%M").to_string();
    let am_pm = now.format("%p").to_string();
    let current_day = now.format("%A").to_string();
    let current_date = now.format("%B %-d").to_string();

    let cpu_temp = cpu_temperature()
        .map(|temp| format!("Pi {:.1}°C", temp))
        .unwrap_or_else(|| "Pi --°C".to_string());

    let network_status = match active_network_interface().as_deref() {
        Some(interface) if interface.starts_with("eth") => "Ethernet • Online",
        Some(interface) if interface.starts_with("wl") => "Wi-Fi • Online",
        Some(_) => "Network • Online",
        None => "Network • Offline",
    };

    let network_color = if network_status.contains("Online") {
        iced::Color::from_rgb8(74, 222, 128)
    } else {
        iced::Color::from_rgb8(248, 113, 113)
    };

    let (weather_temp, weather_condition) = match &_state.weather {
        Some(weather) => (
            format!("{:.0}°F", weather.temperature_2m),
            weather_description(weather.weather_code).to_string(),
        ),
        None => (
            "--°F".to_string(),
            "Weather unavailable".to_string(),
        ),
    };

    let (sunrise, sunset) = match &_state.weather {
        Some(weather) => (
            weather.sunrise
                .as_deref()
                .and_then(format_sun_time)
                .unwrap_or_else(|| "--".to_string()),

            weather.sunset
                .as_deref()
                .and_then(format_sun_time)
                .unwrap_or_else(|| "--".to_string()),
        ),
        None => (
            "--".to_string(),
            "--".to_string(),
        ),
    };

    let time_row = iced::widget::row![
        text(current_time).size(88),
        text(am_pm).size(36),
    ]
        .spacing(8)
        .align_y(iced::Alignment::End);

    let clock_panel = iced::widget::column![
        time_row,
        text(current_day).size(36),
        text(current_date).size(36),
    ]
    .spacing(12)
    .align_x(iced::Alignment::Center);

    let sun_row = iced::widget::row![
        text("↑")
            .size(28)
            .color(iced::Color::from_rgb8(250, 204, 21)),
        text(sunrise).size(28),

        text("↓")
            .size(28)
            .color(iced::Color::from_rgb8(251, 146, 60)),
        text(sunset).size(28),
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center);

    let info_divider = iced::widget::container("")
        .width(iced::Length::Fill)
        .height(1)
        .style(|_| iced::widget::container::Style {
            background: Some(iced::Background::Color(
                iced::Color::from_rgb8(51, 65, 85)
            )),
            ..Default::default()
        });

    let info_panel = iced::widget::column![
        text(weather_temp)
            .size(56)
            .color(iced::Color::from_rgb8(250, 204, 21)),
        text(weather_condition)
            .size(32)
            .color(iced::Color::from_rgb8(203, 213, 225)),
        sun_row,
        info_divider,
        text(cpu_temp)
            .size(42)
            .color(iced::Color::from_rgb8(56, 189, 248)),
        text(network_status)
            .size(32)
            .color(network_color),
    ]
    .spacing(24)
    .align_x(iced::Alignment::Center);

iced::widget::mouse_area(
    iced::widget::container(
        iced::widget::row![
            iced::widget::container(clock_panel)
                .width(iced::Length::FillPortion(3))
                .center_x(iced::Length::Fill)
                .center_y(iced::Length::Fill),
            iced::widget::container("")
                .width(1)
                .height(iced::Length::Fill)
                .style(|_| iced::widget::container::Style {
                    background: Some(iced::Background::Color(
                        iced::Color::from_rgb8(51, 65, 85)
                    )),
                    ..Default::default()
                }),

            iced::widget::container(info_panel)
                .width(iced::Length::FillPortion(2))
                .center_x(iced::Length::Fill)
                .center_y(iced::Length::Fill),
        ]
        .width(iced::Length::Fill)
        .height(iced::Length::Fill)
    )
    .width(iced::Length::Fill)
    .height(iced::Length::Fill)
    .padding(display_padding)
    .style(|_| iced::widget::container::Style {
        background: Some(iced::Background::Color(
            iced::Color::from_rgb8(15, 23, 42)
        )),
        text_color: Some(iced::Color::WHITE),
        ..Default::default()
    })
    )
    .interaction(iced::mouse::Interaction::Hidden)
    .into()
}

fn subscription(_state: &RaspiClock) -> iced::Subscription<Message> {
    let clock = iced::time::every(Duration::from_secs(1))
        .map(|_| Message::Tick);

    let weather = iced::time::every(Duration::from_secs(15 * 60))
        .map(|_| Message::RefreshWeather);

    let burn_in = iced::time::every(Duration::from_secs(5 * 60))
    .map(|_| Message::ShiftDisplay);

    iced::Subscription::batch([
        clock,
        weather,
        burn_in,
    ])
}

fn cpu_temperature() -> Option<f32> {
    let raw = std::fs::read_to_string(
        "/sys/class/thermal/thermal_zone0/temp",
    )
        .ok()?;

    let millidegrees: f32 = raw.trim().parse().ok()?;

    Some(millidegrees / 1000.0)
}

fn active_network_interface() -> Option<String> {
    let output = std::process::Command::new("ip")
        .args(["route", "show", "default"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let route = String::from_utf8(output.stdout).ok()?;
    let parts: Vec<&str> = route.split_whitespace().collect();

    let dev_position = parts.iter().position(|part| *part == "dev")?;

    parts.get(dev_position + 1).map(|interface| interface.to_string())
}

fn fetch_weather(config: &Config) -> Option<CurrentWeather> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&current=temperature_2m,weather_code&daily=sunrise,sunset&temperature_unit=fahrenheit&timezone={}",
        config.latitude,
        config.longitude,
        config.timezone,
    );

    let response = reqwest::blocking::get(&url)
        .ok()?
        .error_for_status()
        .ok()?
        .json::<WeatherResponse>()
        .ok()?;

    let mut current = response.current;

    current.sunrise = response.daily.sunrise.first().cloned();
    current.sunset = response.daily.sunset.first().cloned();

    Some(current)
}

fn weather_description(code: u8) -> &'static str {
    match code {
        0 => "Clear",
        1 => "Mostly Clear",
        2 => "Partly Cloudy",
        3 => "Overcast",
        45 | 48 => "Fog",
        51 | 53 | 55 => "Drizzle",
        56 | 57 => "Freezing Drizzle",
        61 | 63 | 65 => "Rain",
        66 | 67 => "Freezing Rain",
        71 | 73 | 75 => "Snow",
        77 => "Snow Grains",
        80 | 81 | 82 => "Rain Showers",
        85 | 86 => "Snow Showers",
        95 => "Thunderstorm",
        96 | 99 => "Thunderstorm + Hail",
        _ => "Unknown",
    }
}

fn format_sun_time(value: &str) -> Option<String> {
    let datetime =
        chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M").ok()?;

    Some(datetime.format("%-I:%M %p").to_string())
}
