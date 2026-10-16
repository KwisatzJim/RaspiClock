#!/bin/bash
set -euo pipefail

PROJECT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
BINARY="${RASPICLOCK_BINARY:-$PROJECT_DIR/target/release/raspiclock}"
OUTPUT="${RASPICLOCK_OUTPUT:-DSI-1}"
TRANSFORM="${RASPICLOCK_TRANSFORM:-90}"

if [[ ! -x "$BINARY" ]]; then
    echo "RaspiClock executable not found: $BINARY" >&2
    exit 1
fi
command -v cage >/dev/null
if [[ "$TRANSFORM" != "none" ]]; then
    command -v wlr-randr >/dev/null
    command -v timeout >/dev/null
fi

# An inherited socket name can belong to a different compositor. Use an explicit
# override, or discover the new socket created by this Cage session.
RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
export XDG_RUNTIME_DIR="$RUNTIME_DIR"
shopt -s nullglob
declare -A existing_sockets=()
for socket in "$RUNTIME_DIR"/wayland-*; do
    [[ -S "$socket" ]] || continue
    existing_sockets["$socket"]="$(stat -Lc '%d:%i' -- "$socket")"
done
cage -- "$BINARY" &
CAGE_PID=$!
cleanup() {
    kill "$CAGE_PID" 2>/dev/null || true
    wait "$CAGE_PID" 2>/dev/null || true
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

if [[ "$TRANSFORM" != "none" ]]; then
    ready=false
    deadline=$((SECONDS + 10))
    while (( SECONDS < deadline )); do
        kill -0 "$CAGE_PID" 2>/dev/null || break
        candidates=("$RUNTIME_DIR"/wayland-*)
        for socket in "${candidates[@]}"; do
            [[ -S "$socket" ]] || continue
            if [[ -n "${RASPICLOCK_WAYLAND_DISPLAY:-}" ]]; then
                [[ "${socket##*/}" == "$RASPICLOCK_WAYLAND_DISPLAY" ]] || continue
            else
                identity="$(stat -Lc '%d:%i' -- "$socket" 2>/dev/null)" || continue
                [[ "${existing_sockets[$socket]:-}" != "$identity" ]] || continue
            fi
            if WAYLAND_DISPLAY="${socket##*/}" timeout 1s wlr-randr --output "$OUTPUT" --transform "$TRANSFORM"; then
                ready=true
                break 2
            fi
        done
        sleep 0.1
    done
    if [[ "$ready" != true ]]; then
        echo "Cage output did not become ready; check output and transform settings." >&2
        exit 1
    fi
fi
wait "$CAGE_PID"
