#!/bin/bash
# 分析 Voidnix prod 监控日志
# 用法：bash scripts/voidnix-analyze.sh [天数，默认7]
#
# 输出：按天汇总（采样数 / footprint min-avg-max / 漂移 / CPU 峰值 / 线程数 / 数据目录），
#       WebKit XPC 合计趋势，扩展子进程统计，泄漏告警，当日最近 20 个采样点明细。
set -uo pipefail

DAYS="${1:-7}"
LOG_DIR="$HOME/Library/Logs/Voidnix"

echo "=== Voidnix Prod 资源分析（最近 ${DAYS} 天）==="
echo ""

for i in $(seq 0 $((DAYS - 1))); do
  DAY=$(date -v-${i}d '+%Y-%m-%d' 2>/dev/null)
  LOG="$LOG_DIR/monitor-$DAY.log"
  [ -f "$LOG" ] || continue

  # 按天聚合统计（主进程 + WebKit + 扩展子进程）
  STATS=$(grep -v '^#' "$LOG" | awk '
    NF < 3 { next }
    # WebKit 行：& webkit fp_total
    $1 == "&" {
      wk=$3 + 0
      if (wk > 0) {
        wk_n++
        if (!wk_first_done) { wk_first=wk; wk_first_done=1 }
        wk_sum += wk
        if (wk > wk_max) wk_max=wk
        if (wk_min == "" || wk < wk_min) wk_min=wk
        wk_last=wk
      }
      next
    }
    # 扩展子进程行：@ ext/bin rss cpu vsz
    $1 == "@" {
      key=$2; rss=$3 + 0; cpu=$4 + 0
      c_cnt[key]++
      if (c_min[key] == "" || rss < c_min[key]) c_min[key]=rss
      if (rss > c_max[key]) c_max[key]=rss
      if (cpu > c_cpu[key]) c_cpu[key]=cpu
      next
    }
    # 主进程行（NF=5 新格式 fp/cpu/thrd/data，NF=6 旧格式 rss/cpu/thrd/vsz/data）
    NF >= 5 && $2 ~ /^[0-9]/ {
      fp=$2; cpu=$3; thrd=$4
      data = (NF >= 6) ? $6 : $5
      if (!first_done) { first_fp=fp; first_done=1 }
      sum_fp += fp; sum_cpu += cpu; n++
      if (fp > max_fp) max_fp=fp
      if (min_fp == "" || fp < min_fp) min_fp=fp
      if (cpu > max_cpu) max_cpu=cpu
      last_fp=fp
    }
    END {
      if (n == 0) exit
      printf "  samples=%-4d  FP[%.0f ~ %.0f ~ %.0f]  drift=%+.0f  cpu_max=%.1f%%  threads=%d  data=%sMB",
        n, min_fp, sum_fp/n, max_fp, last_fp - first_fp, max_cpu, thrd, data
      if (wk_n > 0) {
        printf "\n  webkit: samples=%-4d  FP[%.0f ~ %.0f ~ %.0f]  drift=%+.0f",
          wk_n, wk_min, wk_sum/wk_n, wk_max, wk_last - wk_first
      }
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

  # 泄漏检测（主进程 FP 漂移 >20MB / WebKit FP 漂移 >50MB）
  DRIFT=$(echo "$STATS" | grep -o 'drift=[+-][0-9.]*' | head -1 | cut -d= -f2)
  WK_DRIFT=$(echo "$STATS" | sed -n 's/.*webkit:.*drift=\([+-][0-9.]*\).*/\1/p')
  [ -n "$DRIFT" ] && awk -v d="$DRIFT" 'BEGIN { exit (d > 20) ? 0 : 1 }' && echo "  [!] 主进程 FP 单日漂移 >20MB"
  [ -n "$WK_DRIFT" ] && awk -v d="$WK_DRIFT" 'BEGIN { exit (d > 50) ? 0 : 1 }' && echo "  [!] WebKit FP 单日漂移 >50MB — compositing layer 累积"
  echo ""
done

# 当日最近采样点（主进程行后合并 WebKit FP）
TODAY=$(date '+%Y-%m-%d')
LOG="$LOG_DIR/monitor-$TODAY.log"
if [ -f "$LOG" ]; then
  echo "=== 今日最近 20 个采样点 ==="
  grep -v '^#' "$LOG" | awk '
    $1 == "&" {
      if (main_buf != "") { print main_buf "  WK=" $3 "MB"; main_buf = "" }
      next
    }
    $1 == "@" { next }
    NF >= 5 {
      if (main_buf != "") print main_buf
      if (NF >= 6)
        main_buf = sprintf("  %s  RSS=%6sMB  CPU=%5s%%  THR=%s  DATA=%sMB", $1, $2, $3, $4, $6)
      else
        main_buf = sprintf("  %s  FP=%6sMB  CPU=%5s%%  THR=%s  DATA=%sMB", $1, $2, $3, $4, $5)
    }
    END { if (main_buf != "") print main_buf }
  ' | tail -20
fi
