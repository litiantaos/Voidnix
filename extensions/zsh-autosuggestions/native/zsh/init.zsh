# zsh-as — Voidnix 终端命令补全
#
# 数据流（无 daemon、无 socket、无 SQLite）：
#   启动：source $ZSH_AS_CACHE → 内存 sorted 数组 + 目录索引（assoc）+ 目录段数组
#   按键：纯内存前缀匹配（当前目录及祖先目录专属列表优先，不足再扫全局 sorted 数组）
#   preexec：记录命令 + 执行时 PWD（cd 类命令归属其发起目录）
#   precmd：append signal log（每条命令一行，5 字段 TSV）+ cache 过期时后台 rebuild
#
# 路径契约：.zshrc 行注入 ZSH_AS_DIR（扩展数据目录），下方 derive 三个子路径。
#   ZSH_AS_DIR      扩展数据目录（.zshrc 注入）
#   ZSH_AS_BIN      binary 路径（= $ZSH_AS_DIR/bin/zsh-autosuggestions）
#   ZSH_AS_CACHE    index.zsh 路径（= $ZSH_AS_DIR/index.zsh）
#   ZSH_AS_SIGNALS  signals.log 路径（= $ZSH_AS_DIR/signals.log）

#--------------------------------------------------------------------#
# 1. Config & globals                                                #
#--------------------------------------------------------------------#

# frecency 依赖每条命令的时间戳（`: <ts>:<dur>;cmd`）。未开时所有命令共享
# 文件 mtime，排序退化为纯频次，多行续行也会被拆成独立命令。
# 仅影响本会话及之后写入的 history；旧记录无 ts 时仍走 mtime fallback。
setopt EXTENDED_HISTORY

: ${ZSH_AS_DIR:=}
ZSH_AS_BIN="$ZSH_AS_DIR/bin/zsh-autosuggestions"
ZSH_AS_CACHE="$ZSH_AS_DIR/index.zsh"
ZSH_AS_SIGNALS="$ZSH_AS_DIR/signals.log"
: ${ZSH_AS_HALF_LIFE_DAYS:=7}
: ${ZSH_AS_FAIL_PENALTY:=0.8}
: ${ZSH_AS_CYCLE_N:=5}
: ${ZSH_AS_HIGHLIGHT_STYLE:=fg=8}
: ${ZSH_AS_BUFFER_MAX_SIZE:=}
: ${ZSH_AS_ORIGINAL_WIDGET_PREFIX:=zsh-as-orig-}

typeset -ga _zsh_autosuggestions_sorted
typeset -gA _zsh_autosuggestions_dir_index
typeset -gi _ZSH_AUTOSUGGESTIONS_IDX_VERSION=0

typeset -g _ZSH_AUTOSUGGESTIONS_LAST_CMD=""
typeset -g _ZSH_AUTOSUGGESTIONS_LAST_PWD=""
typeset -g _ZSH_AUTOSUGGESTIONS_LAST_ACCEPTED=0
typeset -g _ZSH_AUTOSUGGESTIONS_LAST_SUGGESTED=0
typeset -g _ZSH_AUTOSUGGESTIONS_CURRENT_SUGGESTION=""
typeset -ga _ZSH_AUTOSUGGESTIONS_ALTERNATIVES
typeset -gi _ZSH_AUTOSUGGESTIONS_ALT_INDEX=1
typeset -gi _ZSH_AUTOSUGGESTIONS_LAST_REBUILD_AT=0
typeset -gi _ZSH_AUTOSUGGESTIONS_CACHE_MTIME=0

typeset -ga ZSH_AS_ACCEPT_WIDGETS
ZSH_AS_ACCEPT_WIDGETS=(
  forward-char
  end-of-line
  vi-forward-char
  vi-end-of-line
  vi-add-eol
)

typeset -ga ZSH_AS_PARTIAL_ACCEPT_WIDGETS
ZSH_AS_PARTIAL_ACCEPT_WIDGETS=(
  forward-word
  emacs-forward-word
  vi-forward-word
  vi-forward-word-end
  vi-forward-blank-word
  vi-forward-blank-word-end
)

typeset -ga ZSH_AS_CLEAR_WIDGETS
ZSH_AS_CLEAR_WIDGETS=(
  history-search-forward
  history-search-backward
  history-beginning-search-forward
  history-beginning-search-backward
  history-substring-search-up
  history-substring-search-down
  up-line-or-beginning-search
  down-line-or-beginning-search
  up-line-or-history
  down-line-or-history
  copy-earlier-word
)

# 换行执行类 widget：accept-line 直接换行，清空 POSTDISPLAY 变量不擦除
# 屏幕字符 → 建议灰字滞留。走 line_submit action 在 original widget 前强制
# 重绘擦除残留，与 Ctrl+C 表现一致。
typeset -ga ZSH_AS_LINE_SUBMIT_WIDGETS
ZSH_AS_LINE_SUBMIT_WIDGETS=(
  accept-line
  accept-and-hold
  accept-line-and-down-history
)

typeset -ga ZSH_AS_IGNORE_WIDGETS
ZSH_AS_IGNORE_WIDGETS=(
  orig-\*
  beep
  run-help
  set-local-history
  which-command
  yank
  yank-pop
  zle-\*
)

typeset -ga _ZSH_AUTOSUGGESTIONS_BUILTIN_ACTIONS
_ZSH_AUTOSUGGESTIONS_BUILTIN_ACTIONS=(clear fetch suggest accept execute enable disable toggle cycle)

#--------------------------------------------------------------------#
# 2. Cache load (sourceable, <5ms)                                  #
#--------------------------------------------------------------------#

_zsh_autosuggestions_load_cache() {
  [[ -r "$ZSH_AS_CACHE" ]] || return 1
  # source 前重置版本与目录索引，防残留：旧 cache 已缩减的目录不得存活过
  # reload（索引值即段名，连带清上一代段数组，防段数收缩后孤儿数组常驻）
  _ZSH_AUTOSUGGESTIONS_IDX_VERSION=0
  local seg
  for seg in "${_zsh_autosuggestions_dir_index[@]}"; do
    unset "_zsh_autosuggestions_dir_$seg"
  done
  unset _zsh_autosuggestions_dir_index
  typeset -gA _zsh_autosuggestions_dir_index
  source "$ZSH_AS_CACHE"
  # source 后校验格式版本匹配；不匹配视为格式错误，版本归零使 match 拒绝服务直到 rebuild
  if (( _ZSH_AUTOSUGGESTIONS_IDX_VERSION != 2 )); then
    _ZSH_AUTOSUGGESTIONS_IDX_VERSION=0
    return 1
  fi
  return 0
}

# 解析 rebuild 应用的 history 文件：
#   1. 优先 $HISTFILE
#   2. $HISTFILE 为 macOS Terminal 的 session-based `.historynew` 时回落 ~/.zsh_history
#   3. $HISTFILE 未设置时回落 ~/.zsh_history（可读才用）
# 统一 cold start 与 precmd 两条路径的解析逻辑。输出写入 REPLY。
_zsh_autosuggestions_histfile() {
  REPLY=""
  local hf="${HISTFILE:-}"
  if [[ -n "$hf" ]]; then
    if [[ "$hf" == *.historynew ]] && [[ -r "$HOME/.zsh_history" ]]; then
      hf="$HOME/.zsh_history"
    fi
    REPLY="$hf"
    return
  fi
  [[ -r "$HOME/.zsh_history" ]] && REPLY="$HOME/.zsh_history"
}

# 加载缓存；失败（不存在 / 版本或名称漂移）则 rebuild 消除冷启动空窗 / 修复漂移。
# 大 history（>5MB）走异步避免阻塞 shell 启动（precmd 检测 mtime 重载）；小 history 同步（<150ms）并立即重载。
# 后续增量更新由 precmd 异步触发。
() {
  _zsh_autosuggestions_load_cache && return
  _zsh_autosuggestions_histfile
  local hf="$REPLY"
  [[ -n "$hf" && -r "$hf" && -x "$ZSH_AS_BIN" ]] || return
  local -i hf_size=0
  hf_size=$(zstat +size "$hf" 2>/dev/null || echo 0)
  local cmd=(
    "$ZSH_AS_BIN" rebuild
    --out "$ZSH_AS_CACHE"
    --history "$hf"
    --signals "$ZSH_AS_SIGNALS"
    --half-life-days "$ZSH_AS_HALF_LIFE_DAYS"
    --fail-penalty "$ZSH_AS_FAIL_PENALTY"
  )
  if (( hf_size > 5000000 )); then
    "${cmd[@]}" >/dev/null 2>&1 &!
  else
    "${cmd[@]}" >/dev/null 2>&1
    _zsh_autosuggestions_load_cache
  fi
}

#--------------------------------------------------------------------#
# 3. In-memory match (目录优先 + sorted 数组扫描)                    #
#--------------------------------------------------------------------#

_zsh_autosuggestions_match() {
  REPLY=""
  (( _ZSH_AUTOSUGGESTIONS_IDX_VERSION )) || return
  local buf="$1"

  # 空 buffer（新提示符）：仅 PWD 有专属目录段时建议其 top-1，否则不回落——
  # 空提示符建议无前缀约束，祖先/全局兜底只会显示其他项目的命令（跨项目
  # 污染）；无建议是中性态（对齐原版「空 buffer 不打扰」）。非空输入才做
  # 上溯合并与全局兜底。
  if [[ -z "$buf" ]]; then
    if (( ${+_zsh_autosuggestions_dir_index[$PWD]} )); then
      local dref="_zsh_autosuggestions_dir_${_zsh_autosuggestions_dir_index[$PWD]}"
      local -a seg=("${(@P)dref}")
      REPLY="$seg[1]"
    fi
    return
  fi

  # 目录专属列表：PWD 起逐级上溯，各级命中按近→远合并（子目录继承项目根的
  # 常用命令；根 / 不参与——命中即等价全局，无目录语义）
  local -a dir_list=()
  local d="$PWD"
  while [[ "$d" == /* && "$d" != "/" ]]; do
    if (( ${+_zsh_autosuggestions_dir_index[$d]} )); then
      local dref="_zsh_autosuggestions_dir_${_zsh_autosuggestions_dir_index[$d]}"
      dir_list+=("${(@P)dref}")
    fi
    d="${d:h}"
  done

  # ${(b)buf} 转义 glob 元字符，保证字面前缀匹配。
  # 目录命中在前、全局补足在后（两列表各自按 frecency 降序），
  # 共有命令经 (re) 精确下标去重。
  local buf_esc="${(b)buf}"
  local -a results=()
  local cmd
  for cmd in "${dir_list[@]}" "${_zsh_autosuggestions_sorted[@]}"; do
    (( $#results >= $ZSH_AS_CYCLE_N )) && break
    [[ "$cmd" == "$buf_esc"* ]] || continue
    [[ -n "${results[(re)$cmd]}" ]] && continue
    results+=("$cmd")
  done

  (( $#results )) || return
  REPLY="${(F)results}"
}

#--------------------------------------------------------------------#
# 4. Highlighting                                                    #
#--------------------------------------------------------------------#

_zsh_autosuggestions_highlight_reset() {
  typeset -g _ZSH_AUTOSUGGESTIONS_LAST_HIGHLIGHT
  if [[ -n "$_ZSH_AUTOSUGGESTIONS_LAST_HIGHLIGHT" ]]; then
    region_highlight=("${(@)region_highlight:#$_ZSH_AUTOSUGGESTIONS_LAST_HIGHLIGHT}")
    unset _ZSH_AUTOSUGGESTIONS_LAST_HIGHLIGHT
  fi
}

_zsh_autosuggestions_highlight_apply() {
  typeset -g _ZSH_AUTOSUGGESTIONS_LAST_HIGHLIGHT
  if (( $#POSTDISPLAY )); then
    typeset -g _ZSH_AUTOSUGGESTIONS_LAST_HIGHLIGHT="$#BUFFER $(($#BUFFER + $#POSTDISPLAY)) $ZSH_AS_HIGHLIGHT_STYLE"
    region_highlight+=("$_ZSH_AUTOSUGGESTIONS_LAST_HIGHLIGHT")
  else
    unset _ZSH_AUTOSUGGESTIONS_LAST_HIGHLIGHT
  fi
}

#--------------------------------------------------------------------#
# 5. Widget actions                                                  #
#--------------------------------------------------------------------#

_zsh_autosuggestions_disable() {
  typeset -g _ZSH_AUTOSUGGESTIONS_DISABLED
  _zsh_autosuggestions_clear
}

_zsh_autosuggestions_enable() {
  unset _ZSH_AUTOSUGGESTIONS_DISABLED
  (( $#BUFFER )) && _zsh_autosuggestions_fetch
}

_zsh_autosuggestions_toggle() {
  if (( ${+_ZSH_AUTOSUGGESTIONS_DISABLED} )); then
    _zsh_autosuggestions_enable
  else
    _zsh_autosuggestions_disable
  fi
}

_zsh_autosuggestions_clear() {
  POSTDISPLAY=
  _ZSH_AUTOSUGGESTIONS_ALTERNATIVES=()
  _ZSH_AUTOSUGGESTIONS_ALT_INDEX=1
  _ZSH_AUTOSUGGESTIONS_CURRENT_SUGGESTION=""
  _ZSH_AUTOSUGGESTIONS_LAST_SUGGESTED=0
  _zsh_autosuggestions_invoke_original_widget $@
}

_zsh_autosuggestions_modify() {
  local -i retval

  POSTDISPLAY=
  _ZSH_AUTOSUGGESTIONS_ALTERNATIVES=()
  _ZSH_AUTOSUGGESTIONS_ALT_INDEX=1
  _ZSH_AUTOSUGGESTIONS_CURRENT_SUGGESTION=""

  _zsh_autosuggestions_invoke_original_widget $@
  retval=$?

  emulate -L zsh

  (( ${+_ZSH_AUTOSUGGESTIONS_DISABLED} )) && return $retval

  if (( $#BUFFER > 0 )); then
    if [[ -z "$ZSH_AS_BUFFER_MAX_SIZE" ]] || (( $#BUFFER <= $ZSH_AS_BUFFER_MAX_SIZE )); then
      _zsh_autosuggestions_fetch
    fi
  fi

  return $retval
}

_zsh_autosuggestions_line_submit() {
  local -i retval had_suggestion=$#POSTDISPLAY

  POSTDISPLAY=
  _ZSH_AUTOSUGGESTIONS_ALTERNATIVES=()
  _ZSH_AUTOSUGGESTIONS_ALT_INDEX=1
  _ZSH_AUTOSUGGESTIONS_CURRENT_SUGGESTION=""

  # 清空 POSTDISPLAY 变量不等于擦除屏幕显示：accept-line 换行时已渲染的
  # 建议字符不会被擦除而滞留。original widget 前强制重绘，与 Ctrl+C 一致。
  # 仅作用于换行类 widget，回车后立即进入新 ZLE 周期，重绘无副作用。
  (( had_suggestion )) && zle -R

  _zsh_autosuggestions_invoke_original_widget $@
  retval=$?

  return $retval
}

_zsh_autosuggestions_fetch() {
  local suggestion
  _zsh_autosuggestions_match "$BUFFER"
  suggestion="$REPLY"
  _zsh_autosuggestions_suggest "$suggestion"
}

_zsh_autosuggestions_render_suggestion() {
  local s="$1"
  _ZSH_AUTOSUGGESTIONS_CURRENT_SUGGESTION="$s"
  POSTDISPLAY="${s#$BUFFER}"
}

_zsh_autosuggestions_suggest() {
  emulate -L zsh
  local raw="$1"

  _ZSH_AUTOSUGGESTIONS_ALTERNATIVES=("${(@f)raw}")
  _ZSH_AUTOSUGGESTIONS_ALT_INDEX=1
  local suggestion="${_ZSH_AUTOSUGGESTIONS_ALTERNATIVES[1]}"

  if [[ -z "$suggestion" ]] || (( ${+_ZSH_AUTOSUGGESTIONS_DISABLED} )); then
    POSTDISPLAY=
    _ZSH_AUTOSUGGESTIONS_ALTERNATIVES=()
    _ZSH_AUTOSUGGESTIONS_CURRENT_SUGGESTION=""
    _ZSH_AUTOSUGGESTIONS_LAST_SUGGESTED=0
    return
  fi

  _ZSH_AUTOSUGGESTIONS_LAST_SUGGESTED=1
  _zsh_autosuggestions_render_suggestion "$suggestion"
}

_zsh_autosuggestions_cycle() {
  local -i n=${#_ZSH_AUTOSUGGESTIONS_ALTERNATIVES}
  local -i max_cursor_pos=$#BUFFER
  if [[ "$KEYMAP" = "vicmd" ]]; then
    max_cursor_pos=$((max_cursor_pos - 1))
  fi

  if (( CURSOR != max_cursor_pos )); then
    _zsh_autosuggestions_invoke_original_widget expand-or-complete
    return
  fi
  if (( $#POSTDISPLAY == 0 )); then
    _zsh_autosuggestions_invoke_original_widget expand-or-complete
    return
  fi
  # 仅 1 条备选（含空行 top-1 默认建议）：无备选可切换，清建议走补全，
  # 避免 Tab 静默无反应。也修正非空行单备选时 Tab 卡住的既有问题。
  (( n < 2 )) && {
    POSTDISPLAY=
    _ZSH_AUTOSUGGESTIONS_ALTERNATIVES=()
    _ZSH_AUTOSUGGESTIONS_ALT_INDEX=1
    _ZSH_AUTOSUGGESTIONS_CURRENT_SUGGESTION=""
    _zsh_autosuggestions_invoke_original_widget expand-or-complete
    return
  }

  _ZSH_AUTOSUGGESTIONS_ALT_INDEX=$(( (_ZSH_AUTOSUGGESTIONS_ALT_INDEX % n) + 1 ))
  _zsh_autosuggestions_render_suggestion "${_ZSH_AUTOSUGGESTIONS_ALTERNATIVES[$_ZSH_AUTOSUGGESTIONS_ALT_INDEX]}"
}

_zsh_autosuggestions_accept() {
  local -i retval max_cursor_pos=$#BUFFER
  if [[ "$KEYMAP" = "vicmd" ]]; then
    max_cursor_pos=$((max_cursor_pos - 1))
  fi

  if (( $CURSOR != $max_cursor_pos || !$#POSTDISPLAY )); then
    _zsh_autosuggestions_invoke_original_widget $@
    return
  fi

  # 接受 suggestion：置标志位，precmd 时写入 signals.log
  _ZSH_AUTOSUGGESTIONS_LAST_ACCEPTED=1
  _ZSH_AUTOSUGGESTIONS_LAST_SUGGESTED=1

  local accepted="$BUFFER$POSTDISPLAY"
  BUFFER="$accepted"
  POSTDISPLAY=
  _ZSH_AUTOSUGGESTIONS_ALTERNATIVES=()
  _ZSH_AUTOSUGGESTIONS_ALT_INDEX=1
  _ZSH_AUTOSUGGESTIONS_CURRENT_SUGGESTION=""

  _zsh_autosuggestions_invoke_original_widget $@
  retval=$?

  if [[ "$KEYMAP" = "vicmd" ]]; then
    CURSOR=$(($#BUFFER - 1))
  else
    CURSOR=$#BUFFER
  fi

  return $retval
}

_zsh_autosuggestions_execute() {
  BUFFER="$BUFFER$POSTDISPLAY"
  POSTDISPLAY=
  _ZSH_AUTOSUGGESTIONS_ALTERNATIVES=()
  _ZSH_AUTOSUGGESTIONS_ALT_INDEX=1
  _ZSH_AUTOSUGGESTIONS_CURRENT_SUGGESTION=""
  _ZSH_AUTOSUGGESTIONS_LAST_ACCEPTED=1
  _ZSH_AUTOSUGGESTIONS_LAST_SUGGESTED=1
  _zsh_autosuggestions_invoke_original_widget "accept-line"
}

_zsh_autosuggestions_partial_accept() {
  local -i retval cursor_loc
  local original_buffer="$BUFFER"
  local original_suggestion="$_ZSH_AUTOSUGGESTIONS_CURRENT_SUGGESTION"

  _ZSH_AUTOSUGGESTIONS_ALTERNATIVES=()
  _ZSH_AUTOSUGGESTIONS_ALT_INDEX=1
  _ZSH_AUTOSUGGESTIONS_CURRENT_SUGGESTION=""

  BUFFER="$BUFFER$POSTDISPLAY"

  _zsh_autosuggestions_invoke_original_widget $@
  retval=$?

  cursor_loc=$CURSOR
  if [[ "$KEYMAP" = "vicmd" ]]; then
    cursor_loc=$((cursor_loc + 1))
  fi

  if (( $cursor_loc > $#original_buffer )); then
    POSTDISPLAY="${BUFFER[$(($cursor_loc + 1)),$#BUFFER]}"
    BUFFER="${BUFFER[1,$cursor_loc]}"
  else
    BUFFER="$original_buffer"
  fi

  return $retval
}

#--------------------------------------------------------------------#
# 6. Widget wrapping                                                 #
#--------------------------------------------------------------------#

_zsh_autosuggestions_invoke_original_widget() {
  (( $# )) || return 0
  local original_widget_name="$1"
  shift
  if (( ${+widgets[$original_widget_name]} )); then
    zle $original_widget_name -- $@
  fi
}

_zsh_autosuggestions_incr_bind_count() {
  typeset -gi bind_count=$((_ZSH_AUTOSUGGESTIONS_BIND_COUNTS[$1] + 1))
  _ZSH_AUTOSUGGESTIONS_BIND_COUNTS[$1]=$bind_count
}

_zsh_autosuggestions_bind_widget() {
  typeset -gA _ZSH_AUTOSUGGESTIONS_BIND_COUNTS
  local widget=$1
  local zsh_as_action=$2
  local prefix=$ZSH_AS_ORIGINAL_WIDGET_PREFIX
  local -i bind_count

  case $widgets[$widget] in
    user:_zsh_autosuggestions_(bound|orig)_*)
      bind_count=$((_ZSH_AUTOSUGGESTIONS_BIND_COUNTS[$widget]))
      ;;
    user:*)
      _zsh_autosuggestions_incr_bind_count $widget
      zle -N $prefix$bind_count-$widget ${widgets[$widget]#*:}
      ;;
    builtin)
      _zsh_autosuggestions_incr_bind_count $widget
      eval "_zsh_autosuggestions_orig_${(q)widget}() { zle .${(q)widget} }"
      zle -N $prefix$bind_count-$widget _zsh_autosuggestions_orig_$widget
      ;;
    completion:*)
      _zsh_autosuggestions_incr_bind_count $widget
      eval "zle -C $prefix$bind_count-${(q)widget} ${${(s.:.)widgets[$widget]}[2,3]}"
      ;;
  esac

  eval "_zsh_autosuggestions_bound_${bind_count}_${(q)widget}() {
    _zsh_autosuggestions_widget_$zsh_as_action $prefix$bind_count-${(q)widget} \$@
  }"
  zle -N -- $widget _zsh_autosuggestions_bound_${bind_count}_$widget
}

_zsh_autosuggestions_bind_widgets() {
  emulate -L zsh
  local widget
  local -a ignore_widgets
  ignore_widgets=(
    .\*
    _\*
    zsh-as-\*
    $ZSH_AS_ORIGINAL_WIDGET_PREFIX\*
    $ZSH_AS_IGNORE_WIDGETS
  )
  for widget in ${${(f)"$(builtin zle -la)"}:#${(j:|:)~ignore_widgets}}; do
    if [[ -n ${ZSH_AS_CLEAR_WIDGETS[(r)$widget]} ]]; then
      _zsh_autosuggestions_bind_widget $widget clear
    elif [[ -n ${ZSH_AS_LINE_SUBMIT_WIDGETS[(r)$widget]} ]]; then
      _zsh_autosuggestions_bind_widget $widget line_submit
    elif [[ -n ${ZSH_AS_ACCEPT_WIDGETS[(r)$widget]} ]]; then
      _zsh_autosuggestions_bind_widget $widget accept
    elif [[ -n ${ZSH_AS_PARTIAL_ACCEPT_WIDGETS[(r)$widget]} ]]; then
      _zsh_autosuggestions_bind_widget $widget partial_accept
    else
      _zsh_autosuggestions_bind_widget $widget modify
    fi
  done
}

() {
  local action
  for action in $_ZSH_AUTOSUGGESTIONS_BUILTIN_ACTIONS modify partial_accept line_submit; do
    eval "_zsh_autosuggestions_widget_$action() {
      local -i retval
      _zsh_autosuggestions_highlight_reset
      _zsh_autosuggestions_$action \$@
      retval=\$?
      _zsh_autosuggestions_highlight_apply
      zle -R
      return \$retval
    }"
  done
  for action in $_ZSH_AUTOSUGGESTIONS_BUILTIN_ACTIONS; do
    zle -N zsh-as-$action _zsh_autosuggestions_widget_$action
  done
}

#--------------------------------------------------------------------#
# 7. Hooks: signal append + stale rebuild                           #
#--------------------------------------------------------------------#

autoload -Uz add-zsh-hook
zmodload zsh/datetime 2>/dev/null
zmodload zsh/stat 2>/dev/null

_zsh_autosuggestions_preexec() {
  _ZSH_AUTOSUGGESTIONS_LAST_CMD="$1"
  # PWD 取 preexec 时刻：命令在哪个目录敲的就归哪个目录（cd 归属发起目录）
  _ZSH_AUTOSUGGESTIONS_LAST_PWD="$PWD"
}

_zsh_autosuggestions_precmd() {
  local -i exit_code=$?

  _zsh_autosuggestions_histfile
  local hf="$REPLY"

  # append signal（5 字段 TSV：<ts>\t<exit>\t<state>\t<pwd>\t<cmd>）。
  # 每条执行过的命令都记录（目录频次需要正向信号）；空白前缀命令跳过
  # （对齐 HIST_IGNORE_SPACE 的隐私语义）；pwd 异常（相对路径/含控制字符）跳过；
  # histfile 不可读时跳过——signals 唯一消费方 rebuild（含 rotate）以可读
  # history 为前提，此时记录无人消费且永不 rotate，只会无限增长。
  if [[ -n "$hf" && -r "$hf" && -n "$_ZSH_AUTOSUGGESTIONS_LAST_CMD" && "$_ZSH_AUTOSUGGESTIONS_LAST_CMD" != [[:space:]]* ]]; then
    local rec_pwd="$_ZSH_AUTOSUGGESTIONS_LAST_PWD"
    if [[ "$rec_pwd" == /* && "$rec_pwd" != *[[:cntrl:]]* ]]; then
      # strip 所有控制字符（与 Rust 端 is_safe 对齐：拒绝 <0x20 + 0x7f）
      local safe_cmd="${_ZSH_AUTOSUGGESTIONS_LAST_CMD//[[:cntrl:]]/ }"
      local state=0
      (( _ZSH_AUTOSUGGESTIONS_LAST_SUGGESTED )) && state=2
      (( _ZSH_AUTOSUGGESTIONS_LAST_ACCEPTED )) && state=1
      print -r -- "$EPOCHSECONDS"$'\t'"$exit_code"$'\t'"$state"$'\t'"$rec_pwd"$'\t'"$safe_cmd" >> "$ZSH_AS_SIGNALS" 2>/dev/null
    fi
  fi
  _ZSH_AUTOSUGGESTIONS_LAST_CMD=""
  _ZSH_AUTOSUGGESTIONS_LAST_PWD=""
  _ZSH_AUTOSUGGESTIONS_LAST_ACCEPTED=0
  _ZSH_AUTOSUGGESTIONS_LAST_SUGGESTED=0

  # stale 检测：HISTFILE 或 signals.log 比 cache 新 → 后台 rebuild。
  # signals 每条命令都 append（目录频次靠它更新），节流 5 秒防高频回车 fork bomb。
  if [[ -n "$ZSH_AS_BIN" && -x "$ZSH_AS_BIN" ]] && \
     [[ -n "$hf" && -r "$hf" ]] && \
     (( EPOCHSECONDS - _ZSH_AUTOSUGGESTIONS_LAST_REBUILD_AT > 5 )) && \
     { [[ ! -r "$ZSH_AS_CACHE" ]] || [[ "$hf" -nt "$ZSH_AS_CACHE" ]] || [[ "$ZSH_AS_SIGNALS" -nt "$ZSH_AS_CACHE" ]] }; then
    _ZSH_AUTOSUGGESTIONS_LAST_REBUILD_AT=$EPOCHSECONDS
    (
      "$ZSH_AS_BIN" rebuild \
        --out "$ZSH_AS_CACHE" \
        --history "$hf" \
        --signals "$ZSH_AS_SIGNALS" \
        --half-life-days "$ZSH_AS_HALF_LIFE_DAYS" \
        --fail-penalty "$ZSH_AS_FAIL_PENALTY" \
        >/dev/null 2>&1
    ) &!
  fi

  # reload cache：rebuild 是 atomic rename，检测 mtime 变化时重新 source
  if [[ -r "$ZSH_AS_CACHE" ]]; then
    local -i cur_mtime=0
    cur_mtime=$(zstat +mtime "$ZSH_AS_CACHE" 2>/dev/null || echo 0)
    if (( cur_mtime != _ZSH_AUTOSUGGESTIONS_CACHE_MTIME )); then
      _ZSH_AUTOSUGGESTIONS_CACHE_MTIME=$cur_mtime
      _zsh_autosuggestions_load_cache
    fi
  fi
}

add-zsh-hook preexec _zsh_autosuggestions_preexec
add-zsh-hook precmd _zsh_autosuggestions_precmd

#--------------------------------------------------------------------#
# 8. zle-line-init / zle-line-finish (Ctrl+C 拦截)                   #
#--------------------------------------------------------------------#
# Ctrl+C (SIGINT) 不走任何 ZLE widget：ZLE 内部 abort 时清空 BUFFER 与
# POSTDISPLAY 变量，但终端屏幕上 POSTDISPLAY 区域的字符可能未被擦除
# （取决于终端的重绘行为）。解决方案：zle-line-init 时禁用 stty intr，
# 让 ^C 作为普通按键进入 ZLE 触发 widget，在其中清空状态并强制完全重绘；
# zle-line-finish / zshexit 时恢复 intr，保证命令执行期间 ^C 走 SIGINT。

if (( ${+widgets[zle-line-init]} )); then
  case $widgets[zle-line-init] in
    user:_zsh_autosuggestions_*) ;;
    user:*) zle -N _zsh_autosuggestions_orig_line_init ${widgets[zle-line-init]#*:} ;;
    builtin) zle -N _zsh_autosuggestions_orig_line_init .zle-line-init ;;
  esac
fi

if (( ${+widgets[zle-line-finish]} )); then
  case $widgets[zle-line-finish] in
    user:_zsh_autosuggestions_*) ;;
    user:*) zle -N _zsh_autosuggestions_orig_line_finish ${widgets[zle-line-finish]#*:} ;;
    builtin) zle -N _zsh_autosuggestions_orig_line_finish .zle-line-finish ;;
  esac
fi

_zsh_autosuggestions_line_init() {
  (( ${+widgets[_zsh_autosuggestions_orig_line_init]} )) && zle _zsh_autosuggestions_orig_line_init -- "$@"
  # 禁用 stty intr：^C 不再触发 SIGINT，改由 ZLE widget 处理（见 zsh-as-ctrl-c）
  stty intr undef < /dev/tty 2>/dev/null
  # 清空可能的残留状态（保险）
  _zsh_autosuggestions_highlight_reset
  POSTDISPLAY=
  _ZSH_AUTOSUGGESTIONS_ALTERNATIVES=()
  _ZSH_AUTOSUGGESTIONS_ALT_INDEX=1
  _ZSH_AUTOSUGGESTIONS_CURRENT_SUGGESTION=""
  (( ${+_ZSH_AUTOSUGGESTIONS_DISABLED} )) && return
  # 无条件 fetch：空 BUFFER 仅在 PWD 有专属目录段时建议其 top-1（match 内
  # 控制，无段即无建议）；非空 BUFFER（push-line / edit-command-line 重入）
  # 走前缀匹配。
  _zsh_autosuggestions_widget_fetch
}

_zsh_autosuggestions_line_finish() {
  (( ${+widgets[_zsh_autosuggestions_orig_line_finish]} )) && zle _zsh_autosuggestions_orig_line_finish -- "$@"
  # 恢复 stty intr：命令执行期间 ^C 走 SIGINT（中断运行中的命令）
  stty intr '^C' < /dev/tty 2>/dev/null
}

# Ctrl+C widget：清空 suggestion 状态，中断当前行（send-break 自带清 BUFFER + 新行）
_zsh_autosuggestions_ctrl_c() {
  _zsh_autosuggestions_highlight_reset
  POSTDISPLAY=
  _ZSH_AUTOSUGGESTIONS_ALTERNATIVES=()
  _ZSH_AUTOSUGGESTIONS_ALT_INDEX=1
  _ZSH_AUTOSUGGESTIONS_CURRENT_SUGGESTION=""
  _ZSH_AUTOSUGGESTIONS_LAST_SUGGESTED=0
  zle .send-break
}

zle -N zle-line-init _zsh_autosuggestions_line_init
zle -N zle-line-finish _zsh_autosuggestions_line_finish
zle -N zsh-as-ctrl-c _zsh_autosuggestions_ctrl_c

# 安全兜底：zsh 退出时恢复 intr，防终端 ^C 失效
_zsh_autosuggestions_zshexit() {
  stty intr '^C' < /dev/tty 2>/dev/null
}
add-zsh-hook zshexit _zsh_autosuggestions_zshexit

#--------------------------------------------------------------------#
# 9. Startup                                                         #
#--------------------------------------------------------------------#

_zsh_autosuggestions_bind_widgets

_zsh_autosuggestions_apply_keybindings() {
  bindkey '^I' zsh-as-cycle
  bindkey '^X' zsh-as-toggle
  bindkey '^C' zsh-as-ctrl-c
}

_zsh_autosuggestions_apply_keybindings
