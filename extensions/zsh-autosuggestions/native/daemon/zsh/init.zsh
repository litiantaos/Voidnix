# zsh-autosuggestions — 智能命令行预测补全
# 每个 ZLE widget 被包装（非替换），建议通过 POSTDISPLAY + region_highlight 渲染，
# 异步获取通过 `zle -F` 保证按键路径永不阻塞。
#
# BINARY_PATH 在 `init` 命令输出时替换为 daemon 二进制绝对路径。

#--------------------------------------------------------------------#
# 1. Globals & config                                                #
#--------------------------------------------------------------------#

typeset -g ZSH_AS_BIN="{{BINARY_PATH}}"

# 数据目录（daemon 通过 ZSH_AS_DATA_DIR 环境变量读取）
export ZSH_AS_DATA_DIR="{{DATA_DIR}}"

: ${ZSH_AS_HIGHLIGHT_STYLE:=fg=8}
: ${ZSH_AS_USE_ASYNC:=1}
: ${ZSH_AS_MANUAL_REBIND:=}
: ${ZSH_AS_BUFFER_MAX_SIZE:=}
: ${ZSH_AS_ORIGINAL_WIDGET_PREFIX:=zsh-as-orig-}

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
  accept-line
  copy-earlier-word
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

typeset -ga _ZSH_AS_BUILTIN_ACTIONS
_ZSH_AS_BUILTIN_ACTIONS=(clear fetch suggest accept execute enable disable toggle cycle)

typeset -g ZSH_AS_SESSION_ID
if [[ -z "$ZSH_AS_SESSION_ID" ]]; then
  if [[ -r /dev/urandom ]] && (( $+commands[xxd] )); then
    ZSH_AS_SESSION_ID="$(head -c 16 /dev/urandom | xxd -p 2>/dev/null | tr -d '\n')"
  else
    ZSH_AS_SESSION_ID="$$-$RANDOM-$EPOCHSECONDS"
  fi
fi

typeset -g __zsh_as_prev=""
typeset -g __zsh_as_last_cmd=""
typeset -gi __zsh_as_last_start=0

typeset -g _ZSH_AS_CURRENT_SUGGESTION=""

typeset -ga _ZSH_AS_ALTERNATIVES
typeset -gi _ZSH_AS_ALT_INDEX=1

#--------------------------------------------------------------------#
# 2. Daemon auto-spawn                                               #
#--------------------------------------------------------------------#

_zsh_as_ensure_daemon() {
  [[ -x "$ZSH_AS_BIN" ]] || return 1

  "$ZSH_AS_BIN" ping >/dev/null 2>&1 && return 0

  { "$ZSH_AS_BIN" daemon >/dev/null 2>&1 &! } 2>/dev/null

  local -i i
  for (( i = 0; i < 20; i++ )); do
    "$ZSH_AS_BIN" ping >/dev/null 2>&1 && return 0
    sleep 0.01 2>/dev/null || break
  done
  return 1
}

#--------------------------------------------------------------------#
# 3. Highlighting                                                    #
#--------------------------------------------------------------------#

_zsh_as_highlight_reset() {
  typeset -g _ZSH_AS_LAST_HIGHLIGHT

  if [[ -n "$_ZSH_AS_LAST_HIGHLIGHT" ]]; then
    region_highlight=("${(@)region_highlight:#$_ZSH_AS_LAST_HIGHLIGHT}")
    unset _ZSH_AS_LAST_HIGHLIGHT
  fi
}

_zsh_as_highlight_apply() {
  typeset -g _ZSH_AS_LAST_HIGHLIGHT

  if (( $#POSTDISPLAY )); then
    typeset -g _ZSH_AS_LAST_HIGHLIGHT="$#BUFFER $(($#BUFFER + $#POSTDISPLAY)) $ZSH_AS_HIGHLIGHT_STYLE"
    region_highlight+=("$_ZSH_AS_LAST_HIGHLIGHT")
  else
    unset _ZSH_AS_LAST_HIGHLIGHT
  fi
}

#--------------------------------------------------------------------#
# 4. Suggestion fetch                                                #
#--------------------------------------------------------------------#

_zsh_as_fetch_suggestion() {
  local buffer="$1"
  [[ -x "$ZSH_AS_BIN" ]] || return 0
  suggestion="$("$ZSH_AS_BIN" query --buffer "$buffer" --dir "$PWD" --prev "$__zsh_as_prev" --format lines 2>/dev/null)"
}

_zsh_as_async_request() {
  zmodload zsh/system 2>/dev/null

  typeset -g _ZSH_AS_ASYNC_FD _ZSH_AS_CHILD_PID

  if [[ -n "$_ZSH_AS_ASYNC_FD" ]] && { true <&$_ZSH_AS_ASYNC_FD } 2>/dev/null; then
    builtin exec {_ZSH_AS_ASYNC_FD}<&-
    zle -F $_ZSH_AS_ASYNC_FD

    if [[ -n "$_ZSH_AS_CHILD_PID" ]]; then
      if [[ -o MONITOR ]]; then
        kill -TERM -$_ZSH_AS_CHILD_PID 2>/dev/null
      else
        kill -TERM $_ZSH_AS_CHILD_PID 2>/dev/null
      fi
    fi
  fi

  builtin exec {_ZSH_AS_ASYNC_FD}< <(
    echo $sysparams[pid]
    "$ZSH_AS_BIN" query --buffer "$1" --dir "$PWD" --prev "$__zsh_as_prev" --format lines 2>/dev/null
  )

  autoload -Uz is-at-least
  is-at-least 5.8 || command true

  read _ZSH_AS_CHILD_PID <&$_ZSH_AS_ASYNC_FD

  zle -F "$_ZSH_AS_ASYNC_FD" _zsh_as_async_response
}

_zsh_as_async_response() {
  emulate -L zsh

  local suggestion

  if [[ -z "$2" || "$2" == "hup" ]]; then
    IFS='' read -rd '' -u $1 suggestion
    suggestion="${suggestion%$'\n'}"
    zle zsh-as-suggest -- "$suggestion"
    builtin exec {1}<&-
  fi

  zle -F "$1"
  _ZSH_AS_ASYNC_FD=
}

#--------------------------------------------------------------------#
# 5. Widget actions                                                  #
#--------------------------------------------------------------------#

_zsh_as_disable() {
  typeset -g _ZSH_AS_DISABLED
  _zsh_as_clear
}

_zsh_as_enable() {
  unset _ZSH_AS_DISABLED
  (( $#BUFFER )) && _zsh_as_fetch
}

_zsh_as_toggle() {
  if (( ${+_ZSH_AS_DISABLED} )); then
    _zsh_as_enable
  else
    _zsh_as_disable
  fi
}

_zsh_as_clear() {
  POSTDISPLAY=
  _ZSH_AS_ALTERNATIVES=()
  _ZSH_AS_ALT_INDEX=1
  _ZSH_AS_CURRENT_SUGGESTION=""
  _zsh_as_invoke_original_widget $@
}

_zsh_as_modify() {
  local -i retval
  local -i KEYS_QUEUED_COUNT

  local orig_buffer="$BUFFER"
  local orig_postdisplay="$POSTDISPLAY"

  POSTDISPLAY=
  _ZSH_AS_ALTERNATIVES=()
  _ZSH_AS_ALT_INDEX=1
  _ZSH_AS_CURRENT_SUGGESTION=""

  _zsh_as_invoke_original_widget $@
  retval=$?

  emulate -L zsh

  if (( $PENDING > 0 || $KEYS_QUEUED_COUNT > 0 )); then
    POSTDISPLAY="$orig_postdisplay"
    return $retval
  fi

  if [[ "$BUFFER" = "$orig_buffer"* && "$orig_postdisplay" = "${BUFFER:$#orig_buffer}"* ]]; then
    POSTDISPLAY="${orig_postdisplay:$(($#BUFFER - $#orig_buffer))}"
    _ZSH_AS_CURRENT_SUGGESTION="$BUFFER$POSTDISPLAY"
    (( ${+_ZSH_AS_DISABLED} )) || _zsh_as_fetch
    return $retval
  fi

  (( ${+_ZSH_AS_DISABLED} )) && return $retval

  if (( $#BUFFER > 0 )); then
    if [[ -z "$ZSH_AS_BUFFER_MAX_SIZE" ]] || (( $#BUFFER <= $ZSH_AS_BUFFER_MAX_SIZE )); then
      _zsh_as_fetch
    fi
  fi

  return $retval
}

_zsh_as_fetch() {
  if (( ${+ZSH_AS_USE_ASYNC} )) && [[ -n "$ZSH_AS_USE_ASYNC" && "$ZSH_AS_USE_ASYNC" != "0" ]]; then
    _zsh_as_async_request "$BUFFER"
  else
    local suggestion
    _zsh_as_fetch_suggestion "$BUFFER"
    _zsh_as_suggest "$suggestion"
  fi
}

_zsh_as_render_suggestion() {
  local s="$1"
  _ZSH_AS_CURRENT_SUGGESTION="$s"
  POSTDISPLAY="${s#$BUFFER}"
}

_zsh_as_suggest() {
  emulate -L zsh

  local raw="$1"

  _ZSH_AS_ALTERNATIVES=("${(@f)raw}")
  _ZSH_AS_ALT_INDEX=1

  local suggestion="${_ZSH_AS_ALTERNATIVES[1]}"

  if [[ -z "$suggestion" ]] || (( ${+_ZSH_AS_DISABLED} )); then
    POSTDISPLAY=
    _ZSH_AS_ALTERNATIVES=()
    _ZSH_AS_CURRENT_SUGGESTION=""
    return
  fi

  # Safety net: ensure suggestion starts with current buffer (handles async races)
  if (( $#BUFFER > 0 )) && [[ "$suggestion" != "$BUFFER"* ]]; then
    local -i i found=0
    for (( i = 1; i <= ${#_ZSH_AS_ALTERNATIVES}; i++ )); do
      if [[ "${_ZSH_AS_ALTERNATIVES[$i]}" == "$BUFFER"* ]]; then
        suggestion="${_ZSH_AS_ALTERNATIVES[$i]}"
        found=1
        break
      fi
    done
    if (( ! found )); then
      POSTDISPLAY=
      _ZSH_AS_ALTERNATIVES=()
      _ZSH_AS_CURRENT_SUGGESTION=""
      return
    fi
  fi

  _zsh_as_render_suggestion "$suggestion"
}

_zsh_as_cycle() {
  local -i n=${#_ZSH_AS_ALTERNATIVES}
  local -i max_cursor_pos=$#BUFFER

  if [[ "$KEYMAP" = "vicmd" ]]; then
    max_cursor_pos=$((max_cursor_pos - 1))
  fi

  if (( CURSOR != max_cursor_pos )); then
    _zsh_as_invoke_original_widget expand-or-complete
    return
  fi

  if (( $#POSTDISPLAY == 0 )); then
    [[ -n "$_ZSH_AS_ASYNC_FD" ]] && return
    _zsh_as_invoke_original_widget expand-or-complete
    return
  fi

  (( n < 2 )) && return

  _ZSH_AS_ALT_INDEX=$(( (_ZSH_AS_ALT_INDEX % n) + 1 ))
  _zsh_as_render_suggestion "${_ZSH_AS_ALTERNATIVES[$_ZSH_AS_ALT_INDEX]}"
}

_zsh_as_accept() {
  local -i retval max_cursor_pos=$#BUFFER

  if [[ "$KEYMAP" = "vicmd" ]]; then
    max_cursor_pos=$((max_cursor_pos - 1))
  fi

  if (( $CURSOR != $max_cursor_pos || !$#POSTDISPLAY )); then
    _zsh_as_invoke_original_widget $@
    return
  fi

  BUFFER="$BUFFER$POSTDISPLAY"

  POSTDISPLAY=
  _ZSH_AS_ALTERNATIVES=()
  _ZSH_AS_ALT_INDEX=1
  _ZSH_AS_CURRENT_SUGGESTION=""

  _zsh_as_invoke_original_widget $@
  retval=$?

  if [[ "$KEYMAP" = "vicmd" ]]; then
    CURSOR=$(($#BUFFER - 1))
  else
    CURSOR=$#BUFFER
  fi

  return $retval
}

_zsh_as_execute() {
  BUFFER="$BUFFER$POSTDISPLAY"
  POSTDISPLAY=
  _ZSH_AS_ALTERNATIVES=()
  _ZSH_AS_ALT_INDEX=1
  _ZSH_AS_CURRENT_SUGGESTION=""
  _zsh_as_invoke_original_widget "accept-line"
}

_zsh_as_partial_accept() {
  local -i retval cursor_loc
  local original_buffer="$BUFFER"

  _ZSH_AS_ALTERNATIVES=()
  _ZSH_AS_ALT_INDEX=1
  _ZSH_AS_CURRENT_SUGGESTION=""

  BUFFER="$BUFFER$POSTDISPLAY"

  _zsh_as_invoke_original_widget $@
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

_zsh_as_invoke_original_widget() {
  (( $# )) || return 0

  local original_widget_name="$1"
  shift

  if (( ${+widgets[$original_widget_name]} )); then
    zle $original_widget_name -- $@
  fi
}

_zsh_as_incr_bind_count() {
  typeset -gi bind_count=$((_ZSH_AS_BIND_COUNTS[$1] + 1))
  _ZSH_AS_BIND_COUNTS[$1]=$bind_count
}

_zsh_as_bind_widget() {
  typeset -gA _ZSH_AS_BIND_COUNTS

  local widget=$1
  local zsh_as_action=$2
  local prefix=$ZSH_AS_ORIGINAL_WIDGET_PREFIX
  local -i bind_count

  case $widgets[$widget] in
    user:_zsh_as_(bound|orig)_*)
      bind_count=$((_ZSH_AS_BIND_COUNTS[$widget]))
      ;;

    user:*)
      _zsh_as_incr_bind_count $widget
      zle -N $prefix$bind_count-$widget ${widgets[$widget]#*:}
      ;;

    builtin)
      _zsh_as_incr_bind_count $widget
      eval "_zsh_as_orig_${(q)widget}() { zle .${(q)widget} }"
      zle -N $prefix$bind_count-$widget _zsh_as_orig_$widget
      ;;

    completion:*)
      _zsh_as_incr_bind_count $widget
      eval "zle -C $prefix$bind_count-${(q)widget} ${${(s.:.)widgets[$widget]}[2,3]}"
      ;;
  esac

  eval "_zsh_as_bound_${bind_count}_${(q)widget}() {
    _zsh_as_widget_$zsh_as_action $prefix$bind_count-${(q)widget} \$@
  }"

  zle -N -- $widget _zsh_as_bound_${bind_count}_$widget
}

_zsh_as_bind_widgets() {
  emulate -L zsh

  local widget
  local -a ignore_widgets

  ignore_widgets=(
    .\*
    _\*
    ${_ZSH_AS_BUILTIN_ACTIONS/#/zsh-as-}
    $ZSH_AS_ORIGINAL_WIDGET_PREFIX\*
    $ZSH_AS_IGNORE_WIDGETS
  )

  for widget in ${${(f)"$(builtin zle -la)"}:#${(j:|:)~ignore_widgets}}; do
    if [[ -n ${ZSH_AS_CLEAR_WIDGETS[(r)$widget]} ]]; then
      _zsh_as_bind_widget $widget clear
    elif [[ -n ${ZSH_AS_ACCEPT_WIDGETS[(r)$widget]} ]]; then
      _zsh_as_bind_widget $widget accept
    elif [[ -n ${ZSH_AS_PARTIAL_ACCEPT_WIDGETS[(r)$widget]} ]]; then
      _zsh_as_bind_widget $widget partial_accept
    else
      _zsh_as_bind_widget $widget modify
    fi
  done
}

() {
  local action
  for action in $_ZSH_AS_BUILTIN_ACTIONS modify partial_accept; do
    eval "_zsh_as_widget_$action() {
      local -i retval

      _zsh_as_highlight_reset

      _zsh_as_$action \$@
      retval=\$?

      _zsh_as_highlight_apply

      zle -R

      return \$retval
    }"
  done

  for action in $_ZSH_AS_BUILTIN_ACTIONS; do
    zle -N zsh-as-$action _zsh_as_widget_$action
  done
}

#--------------------------------------------------------------------#
# 7. Hooks: record executed commands, re-bind on precmd              #
#--------------------------------------------------------------------#

autoload -Uz add-zsh-hook

_zsh_as_preexec() {
  __zsh_as_last_cmd="$1"
  __zsh_as_last_start=$EPOCHREALTIME
}

_zsh_as_precmd() {
  local -i exit_code=$?

  if [[ -n "$__zsh_as_last_cmd" ]]; then
    local -i duration_ms=0
    if (( __zsh_as_last_start > 0 )); then
      duration_ms=$(( (EPOCHREALTIME - __zsh_as_last_start) * 1000 ))
      (( duration_ms < 0 )) && duration_ms=0
    fi

    if [[ -x "$ZSH_AS_BIN" ]]; then
      { "$ZSH_AS_BIN" record \
        --command "$__zsh_as_last_cmd" \
        --dir "$PWD" \
        --exit "$exit_code" \
        --duration "$duration_ms" \
        --session "$ZSH_AS_SESSION_ID" \
        --prev "$__zsh_as_prev" >/dev/null 2>&1 &! } 2>/dev/null
    fi

    __zsh_as_prev="$__zsh_as_last_cmd"
  fi

  __zsh_as_last_cmd=""
  __zsh_as_last_start=0

  if [[ -z "$ZSH_AS_MANUAL_REBIND" ]]; then
    _zsh_as_bind_widgets
    _zsh_as_apply_keybindings
  fi
}

zmodload zsh/datetime 2>/dev/null

add-zsh-hook preexec _zsh_as_preexec
add-zsh-hook precmd _zsh_as_precmd

#--------------------------------------------------------------------#
# 7b. zle-line-init: fetch on fresh prompts                          #
#--------------------------------------------------------------------#

if (( ${+widgets[zle-line-init]} )); then
  case $widgets[zle-line-init] in
    user:_zsh_as_*) ;;
    user:*) zle -N _zsh_as_orig_line_init ${widgets[zle-line-init]#*:} ;;
    builtin) zle -N _zsh_as_orig_line_init .zle-line-init ;;
  esac
fi

_zsh_as_line_init() {
  (( ${+widgets[_zsh_as_orig_line_init]} )) && zle _zsh_as_orig_line_init -- "$@"

  [[ -n "$BUFFER" ]] && return
  [[ -z "$__zsh_as_prev" ]] && return
  (( ${+_ZSH_AS_DISABLED} )) && return

  _zsh_as_widget_fetch
}

zle -N zle-line-init _zsh_as_line_init

#--------------------------------------------------------------------#
# 8. Startup                                                         #
#--------------------------------------------------------------------#

_zsh_as_ensure_daemon
_zsh_as_bind_widgets

_zsh_as_apply_keybindings() {
  bindkey '^I' zsh-as-cycle
  bindkey '^X' zsh-as-toggle
}

_zsh_as_apply_keybindings
