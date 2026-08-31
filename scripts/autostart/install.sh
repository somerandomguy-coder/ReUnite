#!/usr/bin/env bash
#
# Make this machine join the mesh whenever it is switched on.
#
#   ./scripts/autostart/install.sh            install and start
#   ./scripts/autostart/install.sh --remove   undo it
#
# There is nothing clever here: it runs `meshnet` with no arguments, which is already
# zero-config. The point is only that nobody has to remember to.

set -euo pipefail

cd "$(dirname "$0")/../.."
ROOT="$PWD"
BIN="$ROOT/target/release/meshnet"
LABEL="com.reunite.meshnet"
REMOVE=${1:-}

if [[ "$REMOVE" != "--remove" && ! -x "$BIN" ]]; then
  echo "building the node first (cargo build --release)"
  cargo build --release
fi

case "$(uname)" in
  Darwin)
    PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"
    if [[ "$REMOVE" == "--remove" ]]; then
      launchctl bootout "gui/$(id -u)/$LABEL" 2>/dev/null || true
      rm -f "$PLIST"
      echo "removed $PLIST"
      exit 0
    fi
    mkdir -p "$(dirname "$PLIST")" "$HOME/.meshnet"
    cat > "$PLIST" <<PLIST_EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>$LABEL</string>
  <key>ProgramArguments</key>
  <array>
    <string>$BIN</string>
  </array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>StandardOutPath</key><string>$HOME/.meshnet/meshnet.log</string>
  <key>StandardErrorPath</key><string>$HOME/.meshnet/meshnet.log</string>
</dict>
</plist>
PLIST_EOF
    launchctl bootout "gui/$(id -u)/$LABEL" 2>/dev/null || true
    launchctl bootstrap "gui/$(id -u)" "$PLIST"
    echo "installed $PLIST"
    echo "log: $HOME/.meshnet/meshnet.log"
    ;;

  Linux)
    UNIT="$HOME/.config/systemd/user/meshnet.service"
    if [[ "$REMOVE" == "--remove" ]]; then
      systemctl --user disable --now meshnet.service 2>/dev/null || true
      rm -f "$UNIT"
      systemctl --user daemon-reload 2>/dev/null || true
      echo "removed $UNIT"
      exit 0
    fi
    mkdir -p "$(dirname "$UNIT")"
    cat > "$UNIT" <<UNIT_EOF
[Unit]
Description=ReUnite offline mesh node
After=network.target bluetooth.target

[Service]
ExecStart=$BIN
Restart=always
RestartSec=5

[Install]
WantedBy=default.target
UNIT_EOF
    systemctl --user daemon-reload
    systemctl --user enable --now meshnet.service
    echo "installed $UNIT"
    echo "log: journalctl --user -u meshnet -f"
    echo
    echo "To keep meshing while logged out:  sudo loginctl enable-linger $USER"
    ;;

  *)
    echo "Unsupported platform: $(uname)."
    echo "On Windows, put a shortcut to target\\release\\meshnet.exe in:"
    echo '  %APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup'
    exit 1
    ;;
esac
