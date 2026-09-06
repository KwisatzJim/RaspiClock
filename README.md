# RaspiClock

RaspiClock is a full-screen clock and information dashboard for Raspberry Pi, written in Rust using the Iced GUI framework.

It is designed for small displays and runs directly under Wayland without requiring a full desktop environment.

## Current Features

- Large 12-hour clock with AM/PM
- Day of the week and date
- Current outdoor temperature and weather conditions
- Sunrise and sunset times
- Raspberry Pi CPU temperature
- Network connection status
- Automatic weather updates using Open-Meteo
- High-contrast dark interface designed for readability

## Current Hardware

RaspiClock is currently developed and tested on:

- Raspberry Pi 4 Model B
- 5-inch 800×480 DSI display
- Raspberry Pi OS Lite / Debian 13
- Cage Wayland kiosk compositor

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

Set `latitude` and `longitude` to your location and use the appropriate IANA timezone name.

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
cage -- ./target/release/raspiclock
```

During development, the debug build can be launched with:

```bash
cage -- ./target/debug/raspiclock
```

To stop Cage from another terminal or SSH session:

```bash
pkill cage
```


