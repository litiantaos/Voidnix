#!/bin/bash
# 分析 Voidnix prod 监控日志
# 用法：bash scripts/voidnix-analyze.sh [天数，默认7]
#
# 输出：按天汇总（采样数 / RSS min-avg-max / 漂移 / CPU 峰值 / 线程数 / 数据目录），
#       泄漏告警（单日漂移 >20MB），当日最近 20 个采样点明细。
set -uo pipefail

DAYS="${1:-7}"
LOG_DIR="$HOME/Library/Logs/Voidnix"

echo "=== Voidnix Prod 资源分析（最近 ${DAYS} 天）==="
echo ""

for i in $(seq 0 $((DAYS - 1))); do
  DAY=$(date -v-${i}d '+%Y-%m-%d' 2>/dev/null)
  LOG="$LOG_DIR/monitor-$DAY.log"
  [ -f "$LOG" ] || continue

  # 按天聚合统计（主进程 + 子进程）
  STATS=$(grep -v '^#' "$LOG" | awk '
    NF < 4 { next }
    # 子进程行：@ ext/bin  rss  cpu  vsz
    $1 == "@" {
      key=$2; rss=$3 + 0; cpu=$4 + 0
      c_cnt[key]++
      if (c_min[key] == "" || rss < c_min[key]) c_min[key]=rss
      if (rss > c_max[key]) c_max[key]=rss
      if (cpu > c_cpu[key]) c_cpu[key]=cpu
      next
    }
    # 主进程行
    {
      rss=$2; cpu=$3; thrd=$4; data=$6
      if (!first_done) { first_rss=rss; first_done=1 }
      sum_rss += rss; sum_cpu += cpu; n++
      if (rss > max_rss) max_rss=rss
      if (min_rss == "" || rss < min_rss) min_rss=rss
      if (cpu > max_cpu) max_cpu=cpu
      last_rss=rss
    }
    END {
      if (n == 0) exit
      printf "  samples=%-4d  RSS[%.1f ~ %.1f ~ %.1f]  drift=%+.1f  cpu_max=%.1f%%  threads=%d  data=%sMB",
        n, min_rss, sum_rss/n, max_rss, last_rss - first_rss, max_cpu, thrd, data
      if (length(c_cnt) > 0) {
        nk=0
        for (k in c_cnt) { nk++; keys[nk]=k }
        for (i=1; i<=nk; i++) for (j=i+1; j<=nk; j++) if (keys[i] > keys[j]) { t=keys[i]; keys[i]=keys[j]; keys[j]=t }
        printf "\n  子进程："
        for (i=1; i<=nk; i++) {
          k=keys[i]
          printf "\n    %-32s n=%-4d rss[%.1f ~ %.1f]  cpu_max=%.1f%%", k, c_cnt[k], c_min[k], c_max[k], c_cpu[k]
        }
      }
    }')

  [ -z "$STATS" ] && continue
  echo "$DAY"
  echo "$STATS"

  # 泄漏检测
  DRIFT=$(echo "$STATS" | grep -o 'drift=[+-][0-9.]*' | cut -d= -f2)
  [ -n "$DRIFT" ] && awk -v d="$DRIFT" 'BEGIN { exit (d > 20) ? 0 : 1 }' && echo "  [!] 单日漂移 >20MB — 疑似内存泄漏"
  echo ""
done

# 当日最近采样点
TODAY=$(date '+%Y-%m-%d')
LOG="$LOG_DIR/monitor-$TODAY.log"
if [ -f "$LOG" ]; then
  echo "=== 今日最近 20 个采样点 ==="
  grep -v '^#' "$LOG" | grep -v '^@' | tail -20 | awk '{printf "  %s  RSS=%6sMB  CPU=%5s%%  THR=%s  VSZ=%sMB  DATA=%sMB\n", $1, $2, $3, $4, $5, $6}'
fi
