#!/bin/bash
# 安装/卸载 Voidnix prod 监控 LaunchAgent
# 用法：
#   bash scripts/voidnix-monitor-install.sh           # 安装（登录后自动生效）
#   bash scripts/voidnix-monitor-install.sh uninstall  # 卸载
set -euo pipefail

ACTION="${1:-install}"
LABEL="com.litiantao.voidnix.monitor"
PLIST="$HOME/Library/LaunchAgents/${LABEL}.plist"
SCRIPT="$(cd "$(dirname "$0")" && pwd)/voidnix-monitor.sh"

case "$ACTION" in
  install)
    launchctl unload "$PLIST" 2>/dev/null || true

    cat > "$PLIST" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>${LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>/bin/bash</string>
        <string>${SCRIPT}</string>
    </array>
    <key>StartInterval</key>
    <integer>60</integer>
    <key>RunAtLoad</key>
    <true/>
    <key>StandardOutPath</key>
    <string>/tmp/voidnix-monitor.out</string>
    <key>StandardErrorPath</key>
    <string>/tmp/voidnix-monitor.err</string>
</dict>
</plist>
EOF

    launchctl load "$PLIST"
    echo "[OK] 监控已安装 — 登录后自动运行，每 60s 采样"
    echo "     日志  ~/Library/Logs/Voidnix/monitor-YYYY-MM-DD.log（自动保留 30 天）"
    echo "     分析  bash scripts/voidnix-analyze.sh"
    echo "     卸载  bash scripts/voidnix-monitor-install.sh uninstall"
    ;;
  uninstall)
    launchctl unload "$PLIST" 2>/dev/null || true
    rm -f "$PLIST"
    echo "[OK] 监控已卸载"
    ;;
esac
