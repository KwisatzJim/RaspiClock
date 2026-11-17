# RaspiClock

RaspiClock is a full-screen clock and information dashboard for Raspberry Pi, written in Rust using the Iced GUI framework.

It is designed for small displays and runs directly under Wayland without requiring a full desktop environment.

<img width="2484" height="1426" alt="IMG_2427" src="https://github.com/user-attachments/assets/b72fad5f-fc4d-42a4-a2ce-ade12727ac5c" />


## Current Features

- Large 12-hour clock with AM/PM
- Day of the week and date
- Current outdoor temperature and weather conditions
- Sunrise and sunset times
- Raspberry Pi CPU temperature
- Default-route connection status (IPv4 and IPv6)
- Automatic weather updates using Open-Meteo
- High-contrast dark interface designed for readability

## Current Hardware

RaspiClock is currently developed and tested on:

- Raspberry Pi 4 Model B
- Raspberry Pi Touch Display 2, 7-inch, rotated to landscape at 1280×720
- Raspberry Pi OS Lite / Debian 13
- Cage Wayland kiosk compositor

## Display layout

The dashboard is designed around the current 1280×720 landscape display and scales proportionally with the window size:

- A 60/40 split places the large clock on the left and weather on the right.
- Time digits are 170 px at native resolution, with 40 px AM/PM and 64 px weekday and 56 px date text.
- The weather icon sits beside the temperature, with 56 px conditions and 38 px sunrise/sunset times below, paired with 48 px yellow sunrise and orange sunset SVG icons. The condition has a reserved two-line area for longer descriptions.
- A 56 px footer shows CPU temperature beside a 64 px board SVG and wired connection status beside a 64 px Ethernet SVG. Wi-Fi and other connection types retain their text labels.
- Loading, configuration errors, failed weather updates, and stale-data notices use a reserved area below the weather. Successful updates show no message, and warnings do not move the surrounding content.

The panels share an uninterrupted dark background. A small periodic position shift remains enabled.

## Configuration

RaspiClock reads its configuration from:

```text
~/.config/raspiclock/config.toml
```

Create the configuration directory:

```bash
mkdir -p ~/.config/raspiclock
```

Copy the example configuration:

```bash
cp config.example.toml ~/.config/raspiclock/config.toml
```

Then edit the configuration for your location:

```toml
latitude = 30.27
longitude = -97.74
timezone = "America/Chicago"
```

Set `latitude` and `longitude` to your location and use the appropriate IANA timezone name. Coordinates and timezone names are validated at startup using `/usr/share/zoneinfo` (install `tzdata`). Configuration errors appear on the dashboard; details go to stderr. Restart after changing configuration.

`timezone` controls **weather, sunrise, and sunset only**. The clock and date use the Raspberry Pi's operating-system timezone. To keep both aligned, set the OS timezone too:

```bash
sudo timedatectl set-timezone America/Chicago
```

Weather refreshes every 15 minutes. Failed requests retain the last successful reading and retry after 30 seconds, doubling up to 15 minutes. Requests have a 5-second connection timeout and a 15-second overall timeout. The display distinguishes loading and weather-service errors; weather older than 30 minutes includes its age.

System temperature and default routes are sampled in the background every 30 seconds. “Connected” means a default route exists; it does not prove internet access. Weather-service status reflects the last weather request. The `ip` command (`iproute2`) is required for route detection.

## Building

RaspiClock requires a working Rust toolchain.

Clone the repository:

```bash
git clone https://github.com/KwisatzJim/RaspiClock.git
cd RaspiClock
```

Build the application:

```bash
cargo build --release
```

The compiled executable will be located at:

```text
target/release/raspiclock
```

## Running

RaspiClock is designed to run full-screen using the Cage Wayland kiosk compositor.

From the RaspiClock directory, run:

```bash
./launch-raspiclock.sh
```

The launcher locates the binary relative to itself, waits up to approximately 10 seconds for Cage's Wayland output, then rotates `DSI-1` by 90 degrees. It requires `cage`, `wlr-randr`, and the standard `timeout` command (coreutils). Override settings as needed:

```bash
RASPICLOCK_OUTPUT=HDMI-A-1 RASPICLOCK_TRANSFORM=normal ./launch-raspiclock.sh
RASPICLOCK_TRANSFORM=none ./launch-raspiclock.sh # no rotation; wlr-randr not required
```

`RASPICLOCK_BINARY` overrides the executable path. `RASPICLOCK_WAYLAND_DISPLAY` explicitly selects a Cage socket when automatic discovery is unsuitable. Weather icons are embedded, so the executable can run from any working directory.

During development, the debug build can be launched with:

```bash
cage -- ./target/debug/raspiclock
```

To stop Cage from another terminal or SSH session:

```bash
pkill cage
```

## Automatic Startup

RaspiClock can start automatically on the Raspberry Pi's physical console using Raspberry Pi OS Console Autologin and Fish.

This method has an important advantage: Cage starts from a real `seat0`/`tty1` login session, giving it access to the display and input devices while SSH remains available for remote administration.

### 1. Enable Console Autologin

Run:

```bash
sudo raspi-config
```

Enable console autologin for the user that will run RaspiClock.

After rebooting, the Raspberry Pi should automatically log that user in on `tty1`.

### 2. Configure Fish to launch RaspiClock

Add the following inside the `if status is-interactive` block in:

```text
~/.config/fish/config.fish
```

```fish
if test (tty) = /dev/tty1
    /home/jim/RaspiClock/launch-raspiclock.sh
end
```

Replace `/home/jim/RaspiClock` with the location where you cloned RaspiClock.

The `tty` check restricts automatic launch to the physical `tty1` session. SSH sessions will not launch another copy.

### 3. Reboot

Build the release executable before rebooting:

```bash
cargo build --release
```

Then reboot:

```bash
sudo reboot
```

After console autologin, Fish should automatically start Cage and RaspiClock.

### Remote Recovery

SSH remains available while RaspiClock is running.

To stop Cage and return to the console:

```bash
pkill cage
```

Because Fish launches RaspiClock whenever a new interactive `tty1` Fish session starts, starting another Fish shell on that console will launch RaspiClock again.

To temporarily disable automatic startup, comment out or remove the RaspiClock block from:

```text
~/.config/fish/config.fish
```

## Supervised console startup

For automatic recovery, replace the single launcher invocation in the Fish block with a small supervisor loop. Keep this inside the `tty1` condition so Cage retains the real login session and display permissions:

```fish
while not test -e ~/.config/raspiclock/stop
    /home/jim/RaspiClock/launch-raspiclock.sh >> ~/.config/raspiclock/raspiclock.log 2>&1
    sleep 5
end
```

Create the configuration directory first. The five-second delay prevents a tight restart loop if startup fails. The log contains configuration and weather errors; periodically rotate or truncate it. Stop recovery remotely with:

```bash
touch ~/.config/raspiclock/stop
pkill -x cage
```

Remove the stop file and log in again on `tty1` to resume. With this supervised setup, `pkill cage` alone causes a restart. Disable Console Autologin or remove the Fish block to disable automatic startup permanently.

## Development checks

```bash
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
bash -n launch-raspiclock.sh
python3 -m unittest discover -s tests -v
```

Tests cover configuration validation, response decoding (including polar null sun times), stale-weather boundaries, retry delays, route parsing, and fallback icons. GitHub Actions runs these checks on pushes and pull requests. Display layout and Cage rotation still require a Raspberry Pi hardware check.
