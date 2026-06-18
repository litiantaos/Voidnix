# zsh-autosuggestions

纯 zsh 内核 + stateless rebuild kernel（`extensions/zsh-autosuggestions/native/src/`）。无 SQLite、无 daemon、无 IPC、无 socket、无 launchd。binary 仅做"读 .zsh_history + signals.log → 算 frecency → 写 sourceable zsh cache"，全部 hot path 在 zsh 内存中。

## 数据流

zsh 启动 → `source index.cache`（<5ms，零解析，版本校验）→ 按键纯内存前缀匹配（sorted 数组扫描，前 N 命中即停）→ precmd 钩子 `print >> signals.log`（零 spawn，条件 append）+ stale 检测（`$HISTFILE -nt $ZSH_AS_CACHE`）触发后台 `zsh-as rebuild`。三个路径完全解耦。

## binary 命令（3 个）

- `init`（输出 zsh 集成脚本，无模板替换）
- `rebuild`（读 .zsh_history + signals.log → 先 rotate+compact signals → 写 index.cache，atomic rename）
- `stats`（诊断，支持 `--half-life-days` / `--fail-penalty` 覆盖默认参数；未检测到 extended_history 时提示 `setopt EXTENDED_HISTORY`）

## 保留算法

frecency（`(count+1)^0.7 * exp(-dt/half_life)` + K=10 归一，半衰期默认 7d 可配）+ 前缀匹配（`${(b)buf}` 转义 glob 元字符）+ 失败率惩罚（`sqrt(fail_rate.clamp(1.0)) × fail_penalty`，默认 0.8；clamp 防止 fail_count 逾越 history count 导致 score 钳 0）+ 接受率加权（`0.7 + 0.3 × accept_rate`，accept_rate=1.0 不衰减）。

## 文件布局

`~/Library/Application Support/<bundle-id>/extensions/zsh-autosuggestions/`：

- `index.cache` —— sourceable zsh：`typeset -ga _zsh_as_sorted`（按 score 降序）+ `_ZSH_AS_IDX_VERSION`（zsh 端 source 后校验 `==1`，不匹配则视为格式错误）
- `signals.log` —— append-only TSV：`<exit>\t<state>\t<cmd>`（3 字段；state：0=无 suggestion 互动，1=accepted，2=rejected；仅 `exit!=0 || state!=0` 时 append 控制体积；rebuild 入口 rotate+compact：>1MB 或含无效行时保留最后 10000 有效行 atomic 写回）
- `enabled` —— on/off 标志位
- `bin/zsh-autosuggestions` —— binary（版本号比对复制，见「分发」）
- `bin_version` —— 已部署 binary 版本号（与 binary 同目录）

## history 解析

zsh extended_history 格式 `: <ts>:<dur>;<cmd>`；非 extended 库（无 ts）fallback 用文件 mtime（续行被当作独立命令，已知限制）。多行命令（for/heredoc）折叠为单行（`\n` → 空格）。含控制字符（`< 0x20` 或 `0x7f`）的命令不入 cache（行结构完整性 + 终端安全）。

## cache reload

zsh precmd 用 `zstat +mtime` 检测 cache 变化，变化才重新 source（source 前重置 `_ZSH_AS_IDX_VERSION=0`，source 后校验 `==1`）。冷启动 cache 不存在时同步 rebuild（<5MB history，~150ms）或异步 rebuild（>5MB，避免阻塞启动）。rebuild 节流 5 秒避免高频回车 fork bomb。

## 接受信号采集

`_zsh_as_suggest` 显示 suggestion 时置 `_ZSH_AS_LAST_SUGGESTED=1`（空 suggestion 置 0）；`_zsh_as_accept`/`_zsh_as_execute` 置 `_ZSH_AS_LAST_ACCEPTED=1`。precmd 推导 state（suggested→2，accepted 覆盖→1），仅在有信息量（失败或 suggestion 互动）时 append（cmd 经 `[[:cntrl:]]` strip 与 Rust `is_safe` 对齐），写后清零。精确区分"显示但拒绝"vs"未显示 suggestion"。

## 分发

binary 随主程序 `[[bin]] zsh-autosuggestions` 编译，打入 `.app/Contents/MacOS/`（Tauri 自动打包 `[[bin]]` target）。`on_setup` 用**版本号比对**（编译期常量 `BIN_VERSION` 写入 `bin_version` 文件）从 .app 复制到 `extensions/zsh-autosuggestions/bin/`，已部署版本匹配才跳过；并幂等刷新 .zshrc 行。**改 binary 内容必须 bump `BIN_VERSION`（`mod.rs`），否则不部署——init.zsh 经 `include_str!` 嵌入 binary，改 init.zsh 也算改 binary。** .zshrc 行：`export ZSH_AS_BIN=... ZSH_AS_CACHE=... ZSH_AS_SIGNALS=...; eval "$("$ZSH_AS_BIN" init)"`（行尾 marker `# voidnix zsh-autosuggestions` 用于精确 remove）。.zshrc 写入走原子 tmp+rename + `.zshrc.voidnix-bak` 备份。关闭扩展时清理 `index.cache` + `signals.log`（保留 binary 避免反复复制），`set_zsh_autosuggestions_enabled` 返回 `Result`，失败时前端 revert 状态并 `showStatus` 提示。

## history 路径解析

zsh 端 `_zsh_as_histfile` 统一解析 rebuild 目标 history：优先 `$HISTFILE`；`.historynew`（macOS Terminal session 副本）或 `$HISTFILE` 未设置时回落 `~/.zsh_history`。cold start 与 precmd stale 检测共用。

## 并发

`SETUP_LOCK` 串行化 `on_setup` / `set_zsh_autosuggestions_enabled` 路径，poison 时也恢复。

## Ctrl+C 拦截

Ctrl+C（SIGINT）不走任何 ZLE widget，POSTDISPLAY 会残留在重绘的新行；且 POSTDISPLAY 是 ZLE 特殊变量，TRAPINT（非 widget 上下文）中只读无法修改。解决方案：`zle-line-init` 时 `stty intr undef` 让 `^C` 作为普通按键进入 ZLE，绑定 `zsh-as-ctrl-c` widget（清空 POSTDISPLAY/高亮/状态 + `zle .send-break` 中断当前行）；`zle-line-finish` / `zshexit` 恢复 `stty intr '^C'` 保证命令执行期间 `^C` 走 SIGINT。
