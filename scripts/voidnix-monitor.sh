#!/bin/bash
# Voidnix prod 采样器（launchd 每 60s 调用一次）
# 进程未运行时 < 10ms 退出零开销；运行时单次 ps 采样追加到当日日志。
set -uo pipefail

LOG_DIR="$HOME/Library/Logs/Voidnix"
DATA_DIR="$HOME/Library/Application Support/com.litiantao.voidnix"
TODAY=$(date '+%Y-%m-%d')
LOG="$LOG_DIR/monitor-$TODAY.log"

mkdir -p "$LOG_DIR"

# 清理 30 天前日志（后台不阻塞）
find "$LOG_DIR" -name "monitor-*.log" -mtime +30 -delete 2>/dev/null &

# 找 prod 进程（安装版优先，release 直跑次选，排除 debug）
PID=$(pgrep -f "/Applications/Voidnix.app/Contents/MacOS/Voidnix" 2>/dev/null | head -1)
[ -z "$PID" ] && PID=$(pgrep -f "target/release/Voidnix" 2>/dev/null | head -1)
[ -z "$PID" ] && exit 0

# 新日志文件写表头
if [ ! -f "$LOG" ]; then
  printf "# Voidnix Prod Monitor %s\n# time  rss_mb  cpu%%  threads  vsz_mb  data_mb\n" "$TODAY" >> "$LOG"
fi

# 单次 ps 取 rss/cpu/vsz（KB）
INFO=$(ps -o rss=,%cpu=,vsz= -p "$PID" 2>/dev/null | tr -s ' ')
[ -z "$INFO" ] && exit 0
read -r RSS_KB CPU VSZ_KB <<< "$INFO"

# 线程数
THRD=$(ps -M -p "$PID" 2>/dev/null | wc -l | awk '{print $1 - 1}')
[ -z "$THRD" ] || [ "$THRD" -lt 0 ] 2>/dev/null && THRD=0

# 数据目录大小（MB）
DATA_MB=$(du -sm "$DATA_DIR" 2>/dev/null | awk '{print $1}')
[ -z "$DATA_MB" ] && DATA_MB="-"

# 格式化输出（awk 做浮点除法，避免依赖 bc）
awk -v t="$(date '+%H:%M:%S')" -v r="$RSS_KB" -v c="$CPU" -v n="$THRD" -v v="$VSZ_KB" -v d="$DATA_MB" \
  'BEGIN { printf "%s  %.1f  %s  %d  %.1f  %s\n", t, r/1024, c, n, v/1048576, d }' >> "$LOG"
