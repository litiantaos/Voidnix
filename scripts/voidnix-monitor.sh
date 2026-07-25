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
  printf "# Voidnix Prod Monitor %s\n# time  rss_mb  cpu%%  threads  vsz_mb  data_mb\n# @ ext/bin  rss_mb  cpu%%  vsz_mb   (扩展子进程，紧随主进程行)\n" "$TODAY" >> "$LOG"
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

# 扩展子进程采样（路径匹配数据目录，不依赖 PPID 链——root 子进程如 mihomo 已 reparent 到 launchd）
# 一次 ps 全表扫描 + awk，识别 comm 含 com.litiantao.voidnix/extensions/<id>/ 的进程，按扩展分组
ps -A -o rss=,%cpu=,vsz=,command= 2>/dev/null | awk '
  NF < 4 { next }
  {
    line=$0
    # 字面量拆分拼接：避免 awk 源码出现完整 marker，防止 ps 扫到 awk 自身进程
    marker="com.litiantao.void" "nix/extensions/"
    ml=length(marker)
    p=index(line, marker)
    if (p == 0) next
    rest=substr(line, p + ml)
    sl=index(rest, "/")
    if (sl == 0) next
    ext=substr(rest, 1, sl - 1)
    # ext 白名单：扩展 id 仅小写字母/数字/连字符，过滤命令行含 extensions/<id>
    # 字样但非真实扩展子进程的多行命令（排查 shell / osascript 包装脚本，换行
    # 渲染为 \012 致 ext 吞掉引号与后续语句）
    if (ext !~ /^[a-z0-9-]+$/) next
    after=substr(rest, sl + 1)
    sp=index(after, " ")
    if (sp > 0) binpath=substr(after, 1, sp - 1); else binpath=after
    nb=split(binpath, parts, "/")
    bin=parts[nb]
    printf "@ %s/%s  %.1f  %s  %.1f\n", ext, bin, $1/1024, $2, $3/1048576
  }' >> "$LOG"
