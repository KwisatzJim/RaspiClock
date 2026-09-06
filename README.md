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
if test "$TERM" = "linux"
    exec /usr/bin/cage -- /home/jim/RaspiClock/target/release/raspiclock
end
```

Replace `/home/jim/RaspiClock` with the location where you cloned RaspiClock.

The `$TERM = "linux"` check restricts the automatic launch to the local Linux console. SSH sessions normally use a different terminal type, so connecting over SSH will not start another copy of RaspiClock.

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


