"""Exercise the launcher without a display or Cage installation."""
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import unittest

LAUNCHER = Path(__file__).resolve().parents[1] / "launch-raspiclock.sh"


class LauncherTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.env = dict(os.environ, PATH=f"{self.root}:{os.environ['PATH']}",
                        XDG_RUNTIME_DIR=str(self.root), RASPICLOCK_TRANSFORM="none",
                        RASPICLOCK_BINARY=str(self.root / "clock"))
        self.env.pop("RASPICLOCK_WAYLAND_DISPLAY", None)
        self.executable("clock", "#!/bin/sh\nexit 0\n")

    def executable(self, name, contents):
        path = self.root / name
        path.write_text(contents)
        path.chmod(0o755)

    def run_launcher(self):
        return subprocess.run([str(LAUNCHER)], env=self.env, cwd="/tmp",
                              capture_output=True, text=True, timeout=15)

    def test_missing_binary_is_explained(self):
        self.env["RASPICLOCK_BINARY"] = str(self.root / "missing")
        result = self.run_launcher()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("executable not found", result.stderr)

    def test_no_rotation_preserves_compositor_exit_status(self):
        self.executable("cage", '#!/bin/sh\n[ "$1" = "--" ] || exit 99\nexit 7\n')
        self.assertEqual(self.run_launcher().returncode, 7)

    def test_discovers_socket_and_applies_output_settings(self):
        self.executable("cage", """#!/usr/bin/env python3
import os, socket, time
sock = socket.socket(socket.AF_UNIX)
sock.bind(os.path.join(os.environ['XDG_RUNTIME_DIR'], 'wayland-3'))
time.sleep(0.8)
""")
        self.executable("wlr-randr", """#!/bin/sh
[ "$WAYLAND_DISPLAY" = "wayland-3" ] &&
[ "$*" = "--output HDMI-A-1 --transform 180" ] || exit 8
printf success > "$XDG_RUNTIME_DIR/rotated"
""")
        self.env.update(RASPICLOCK_TRANSFORM="180", RASPICLOCK_OUTPUT="HDMI-A-1")
        result = self.run_launcher()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.root / "rotated").read_text(), "success")

    def test_reused_stale_socket_path_is_detected(self):
        stale = socket.socket(socket.AF_UNIX)
        self.addCleanup(stale.close)
        stale.bind(str(self.root / "wayland-0"))
        self.executable("cage", """#!/usr/bin/env python3
import os, socket, time
path = os.path.join(os.environ['XDG_RUNTIME_DIR'], 'wayland-0')
os.unlink(path)
sock = socket.socket(socket.AF_UNIX)
sock.bind(path)
time.sleep(0.8)
""")
        self.executable("wlr-randr", '#!/bin/sh\n[ "$WAYLAND_DISPLAY" = "wayland-0" ]\n')
        self.env["RASPICLOCK_TRANSFORM"] = "90"
        result = self.run_launcher()
        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
