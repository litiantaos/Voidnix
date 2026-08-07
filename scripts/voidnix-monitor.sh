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
  printf "# Voidnix Prod Monitor %s\n# time  fp_mb  cpu%%  threads  data_mb\n# & webkit  fp_total_mb  (WebKit XPC 合计，按启动时间关联)\n# @ ext/bin  rss_mb  cpu%%  vsz_mb   (扩展子进程，紧随主进程行)\n" "$TODAY" >> "$LOG"
fi

# CPU + 线程数（ps，与下方抓栈逻辑一致）
CPU=$(ps -o %cpu= -p "$PID" 2>/dev/null | tr -d ' ')
[ -z "$CPU" ] && exit 0
THRD=$(ps -M -p "$PID" 2>/dev/null | wc -l | awk '{print $1 - 1}')
[ -z "$THRD" ] || [ "$THRD" -lt 0 ] 2>/dev/null && THRD=0

# Physical footprint（top mem = 物理内存足迹，含被内核压缩的内存页；ps rss 不含，严重低估 WKWebView 进程）
FP_MB=$(top -l 1 -pid "$PID" -stats pid,mem 2>/dev/null | awk '
  $1 ~ /^[0-9]+$/ && $NF ~ /^[0-9.]+[KMG]$/ {
    m=$NF
    if (m~/M$/) v=substr(m,1,length(m)-1)+0
    else if (m~/K$/) v=substr(m,1,length(m)-1)/1024
    else v=(substr(m,1,length(m)-1)+0)*1024
    printf "%.0f", v
  }')
[ -z "$FP_MB" ] && FP_MB="-"

# 数据目录大小（MB）
DATA_MB=$(du -sm "$DATA_DIR" 2>/dev/null | awk '{print $1}')
[ -z "$DATA_MB" ] && DATA_MB="-"

printf "%s  %s  %s  %s  %s\n" "$(date '+%H:%M:%S')" "$FP_MB" "$CPU" "$THRD" "$DATA_MB" >> "$LOG"

# WebKit XPC 子进程 footprint 合计（与主进程同时启动 ±10s 的 com.apple.WebKit.* 进程）
# ps rss 对 WKWebView 严重失真（如 WebContent 进程 ps 报 47M / 实际 footprint 175M），
# 必须用 top footprint 才能反映真实占用
MAIN_LS=$(ps -o lstart= -p "$PID" 2>/dev/null | xargs)
MAIN_EP=$(date -j -f "%a %b %d %H:%M:%S %Y" "$MAIN_LS" +%s 2>/dev/null)
if [ -n "$MAIN_EP" ] && [ "$MAIN_EP" -gt 0 ] 2>/dev/null; then
  WK_PIDS=""
  while read -r wp _dow _mon _day _time _year _rest; do
    we=$(date -j -f "%a %b %d %H:%M:%S %Y" "$_dow $_mon $_day $_time $_year" +%s 2>/dev/null)
    [ -n "$we" ] && {
      d=$((we - MAIN_EP))
      [ "$d" -ge -10 ] && [ "$d" -le 10 ] && WK_PIDS="$WK_PIDS -pid $wp"
    }
  done < <(ps -ax -o pid=,lstart=,comm= 2>/dev/null | grep "com.apple.WebKit")
  if [ -n "$WK_PIDS" ]; then
    WK_FP=$(top -l 1 $WK_PIDS -stats pid,mem 2>/dev/null | awk '
      $1 ~ /^[0-9]+$/ && $NF ~ /^[0-9.]+[KMG]$/ {
        m=$NF
        if (m~/M$/) v=substr(m,1,length(m)-1)+0
        else if (m~/K$/) v=substr(m,1,length(m)-1)/1024
        else v=(substr(m,1,length(m)-1)+0)*1024
        total+=v
      }
      END { if (total>0) printf "%.0f", total }')
    [ -n "$WK_FP" ] && printf "& webkit  %s\n" "$WK_FP" >> "$LOG"
  fi
fi

# 扩展子进程采样（不依赖 PPID 链——root 子进程如 mihomo 由 launchd LaunchDaemon 托管，PPID=1）
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
