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

# 扩展子进程采样（不依赖 PPID 链——root 子进程如 mihomo 已 reparent 到 launchd）
# 用 comm（可执行文件路径）而非 command（完整命令行）匹配：只有可执行确实位于
# extensions/<id>/ 下的进程才命中，grep/osascript/采样器 fork 的 shell 等仅在参数里
# 引用该路径的进程天然排除，从根上杜绝误匹配（无需靠 ext 白名单兜底）
ps -A -o rss=,%cpu=,vsz=,comm= 2>/dev/null | awk '
  NF < 4 { next }
  {
    line=$0
    # 字面量拆分拼接：避免 awk 源码出现完整 marker
    marker="com.litiantao.void" "nix/extensions/"
    ml=length(marker)
    p=index(line, marker)
    if (p == 0) next
    rest=substr(line, p + ml)
    sl=index(rest, "/")
    if (sl == 0) next
    ext=substr(rest, 1, sl - 1)
    # ext 白名单（二道防线）：扩展 id 仅小写字母/数字/连字符
    if (ext !~ /^[a-z0-9-]+$/) next
    # comm 末段无参数，binpath 取 marker 后首个 / 到行尾，可含空格（如 awake 的 "Display Wakelock"）
    binpath=substr(rest, sl + 1)
    sub(/[ \t]+$/, "", binpath)
    nb=split(binpath, parts, "/")
    bin=parts[nb]
    # bin 名校验：排除含引号/换行/分号等特殊字符的异常值
    # （极端情况下 comm 输出跨行残留会拼入 bin 名，如 `config.yaml"<LF>grep`）
    if (bin !~ /^[A-Za-z0-9._ -]+$/) next
    printf "@ %s/%s  %.1f  %s  %.1f\n", ext, bin, $1/1024, $2, $3/1048576
  }' >> "$LOG"

# ── CPU 阈值触发抓栈（外部 sample，零侵入主进程代码）──
# 主进程瞬时 CPU 超阈值时抓调用栈，定位高占用根因；
# 冷却窗口防连续尖峰刷屏；快照独立存 stacks/，主日志仅追加 # [stack] 关联行（被 analyze 跳过）
CPU_THRESHOLD=80         # CPU% 触发阈值
STACK_COOLDOWN=300       # 冷却秒数（同一尖峰窗口内只抓一次）
STACK_DIR="$LOG_DIR/stacks"
STACK_STATE="$LOG_DIR/.cpu-stack-last"
if awk -v c="$CPU" -v t="$CPU_THRESHOLD" 'BEGIN{exit !(c+0 >= t)}'; then
  NOW=$(date +%s)
  LAST=$(cat "$STACK_STATE" 2>/dev/null)
  case "$LAST" in ''|*[!0-9]*) LAST=0 ;; esac
  if [ $((NOW - LAST)) -ge "$STACK_COOLDOWN" ]; then
    mkdir -p "$STACK_DIR"
    STAMP=$(date '+%Y%m%d-%H%M%S')
    # sample -file 指定输出文件（避免 sample 默认额外往 /tmp 写 .sample.txt）；-mayDie 允许进程期间退出
    if sample "$PID" 2 -mayDie -file "$STACK_DIR/cpu-$STAMP.txt" >/dev/null 2>&1; then
      printf '%s' "$NOW" > "$STACK_STATE"
      printf '# [stack] %s cpu=%s -> stacks/cpu-%s.txt\n' "$(date '+%H:%M:%S')" "$CPU" "$STAMP" >> "$LOG"
    fi
    # 清理 30 天前抓栈快照（后台不阻塞）
    find "$STACK_DIR" -name "cpu-*.txt" -mtime +30 -delete 2>/dev/null &
  fi
fi
