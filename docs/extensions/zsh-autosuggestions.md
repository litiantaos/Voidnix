# zsh-autosuggestions

纯 zsh 内核 + stateless rebuild kernel（`extensions/zsh-autosuggestions/native/src/`）。无 SQLite、无 daemon、无 IPC、无 socket、无 launchd。binary 仅做"读 .zsh_history + signals.log → 算 frecency → 写 sourceable zsh cache"，全部 hot path 在 zsh 内存中。

## 数据流

zsh 启动 → `source index.zsh`（<5ms，零解析，版本校验）→ 按键纯内存前缀匹配（sorted 数组扫描，前 N 命中即停）→ precmd 钩子 `print >> signals.log`（零 spawn，条件 append）+ stale 检测（`$HISTFILE -nt $ZSH_AS_CACHE`）触发后台 `zsh-as rebuild`。三个路径完全解耦。

> **命名约定**：zsh 内部函数/变量统一 `_zsh_autosuggestions_*`（函数）与 `_ZSH_AUTOSUGGESTIONS_*`（全局变量）；`ZSH_AS_DIR` 是 .zshrc 注入的**环境变量契约**（外部接口），init.zsh 据此 derive `ZSH_AS_BIN/CACHE/SIGNALS`（内部保留短名）；widget 名 `zsh-as-*`（保留）。

## binary 命令（3 个）

- `init`（输出 zsh 集成脚本，无模板替换，`include_str!` 嵌入）
- `rebuild`（读 .zsh_history + signals.log → 先 rotate+compact signals → 写 index.zsh，atomic rename）
- `stats`（诊断，支持 `--half-life-days` / `--fail-penalty` 覆盖默认参数；未检测到 extended_history 时提示——通常是历史文件仍为旧格式，新会话已由 init 自动 `setopt EXTENDED_HISTORY`）

## 保留算法

frecency（`(count+1)^0.7 * exp(-dt/half_life)` + K=10 归一，半衰期默认 7d 可配）+ 前缀匹配（`${(b)buf}` 转义 glob 元字符）+ 失败率惩罚（`sqrt(fail_rate.clamp(1.0)) × fail_penalty`，默认 0.8；clamp 防止 fail_count 逾越 history count 导致 score 钳 0）+ 接受率加权（`0.7 + 0.3 × accept_rate`，accept_rate=1.0 不衰减）。

## 文件布局

`~/Library/Application Support/<bundle-id>/extensions/zsh-autosuggestions/`：

- `index.zsh` —— sourceable zsh：`typeset -ga _zsh_autosuggestions_sorted`（按 score 降序）+ `typeset -gi _ZSH_AUTOSUGGESTIONS_IDX_VERSION`（zsh 端 source 后校验 `==1`，不匹配则视为格式错误）
- `signals.log` —— append-only TSV：`<exit>\t<state>\t<cmd>`（3 字段；state：0=无 suggestion 互动，1=accepted，2=rejected；仅 `exit!=0 || state!=0` 时 append 控制体积；rebuild 入口 rotate+compact：>1MB 或含无效行时保留最后 10000 有效行 atomic 写回）
- `enabled` —— on/off 标志位
- `bin/zsh-autosuggestions` —— binary（版本号比对复制，见「分发」）
- `bin.version` —— 已部署 binary 版本号（与 binary 同目录）

## history 解析

zsh extended_history 格式 `: <ts>:<dur>;<cmd>`。`init.zsh` 启动时自动 `setopt EXTENDED_HISTORY`（启用扩展即写带时间戳的 history；关闭扩展后不再注入）。非 extended 库（无 ts，常见于开启前的旧文件）fallback 用文件 mtime（续行被当作独立命令，已知限制）。多行命令（for/heredoc）折叠为单行（`\n` → 空格）。含控制字符（`< 0x20` 或 `0x7f`）的命令不入 cache（行结构完整性 + 终端安全）。

## cache reload

zsh precmd 用 `zstat +mtime` 检测 cache 变化，变化才重新 source（source 前重置 `_ZSH_AUTOSUGGESTIONS_IDX_VERSION=0`，source 后校验 `==1`）。冷启动 cache 不存在时同步 rebuild（<5MB history，~150ms）或异步 rebuild（>5MB，避免阻塞启动）。rebuild 节流 5 秒避免高频回车 fork bomb。

## 接受信号采集

`_zsh_autosuggestions_suggest` 显示 suggestion 时置 `_ZSH_AUTOSUGGESTIONS_LAST_SUGGESTED=1`（空 suggestion 置 0）；`_zsh_autosuggestions_accept` / `_zsh_autosuggestions_execute` 置 `_ZSH_AUTOSUGGESTIONS_LAST_ACCEPTED=1`。precmd 推导 state（suggested→2，accepted 覆盖→1），仅在有信息量（失败或 suggestion 互动）时 append（cmd 经 `[[:cntrl:]]` strip 与 Rust `is_safe` 对齐），写后清零。精确区分"显示但拒绝"vs"未显示 suggestion"。

## 分发

binary 是独立 `[[bin]]` target（`Cargo.toml` 声明，path 指向 `native/src/main.rs`）。**Tauri 不自动打包额外 `[[bin]]`**，需手动编译 + 嵌入：

- **dev**：`package.json` 的 `tauri:dev` 前置 `build:zsh-bin`（`cargo build --manifest-path extensions/zsh-autosuggestions/native/Cargo.toml`），产物在 `extensions/zsh-autosuggestions/native/target/debug/zsh-autosuggestions`。
- **release**：`deploy.sh` 调 `bun run tauri build`（内部 `prebuild:zsh` 先 `cargo build --release --manifest-path extensions/zsh-autosuggestions/native/Cargo.toml`，`CARGO_TARGET_DIR` 指向 `src-tauri/target`），binary 经 `tauri.conf.json` 的 `bundle.resources` 自动嵌入 `Voidnix.app/Contents/Resources/zsh-autosuggestions`。

`source_bin()` 双路径定位（P3-8 同步）：

1. 优先 `current_exe().parent().join("zsh-autosuggestions")`（dev = `target/debug/`）
2. 兜底 `current_exe().parent().parent().join("Resources").join(...)`（release = `.app/Contents/Resources/`）

**版本号部署**：`install_bin` 比较 `bin.version` 文件与编译期常量 `BIN_VERSION`（`mod.rs`），相等即跳过复制。**每改 binary 内容（`native/src/*.rs` 或 `include_str!` 嵌入的 `init.zsh`）必须 bump `BIN_VERSION`——开发期迭代亦然**：同一版本号只在首次部署时复制产物，共用版本号会导致改动不部署。`bin.version` 与 binary 同目录，缺失视为 0。手动改 init.zsh 后立即生效可重启 `tauri:dev`，或手动复制 `src-tauri/target/debug/zsh-autosuggestions` + 写 `bin.version`。

`setup` 并幂等刷新 .zshrc 行。

.zshrc 写入两行：marker 注释行 `# voidnix zsh-autosuggestions` + `export ZSH_AS_DIR=...; eval "$("$ZSH_AS_DIR/bin/zsh-autosuggestions" init)"`，块上下各留一个空行与文件其余内容分隔。子路径（bin/cache/signals）由 init.zsh 从 `ZSH_AS_DIR` derive。摘除按 marker 行 + 紧跟 export 行 + 相邻空行整体删除（避免重复刷新累积空行）。写入走原子 tmp+rename + `.zshrc.voidnix-bak` 备份。关闭扩展时清理 `index.zsh` + `signals.log` + `.zshrc.voidnix-bak`（保留 binary 避免反复复制）。

`View.vue` toggle **显式 invoke** `set_zsh_autosuggestions_enabled`，成功才更新 `config.enabled`，失败 `showStatus` 提示（避免 config 与 `enabled_flag` 不一致）。

## history 路径解析

zsh 端 `_zsh_autosuggestions_histfile` 统一解析 rebuild 目标 history：优先 `$HISTFILE`；`.historynew`（macOS Terminal session 副本）或 `$HISTFILE` 未设置时回落 `~/.zsh_history`。cold start 与 precmd stale 检测共用。

## 并发

`SETUP_LOCK` 串行化 `setup` / `set_zsh_autosuggestions_enabled` 路径，poison 时也恢复。

## Ctrl+C 拦截

Ctrl+C（SIGINT）不走任何 ZLE widget，POSTDISPLAY 会残留在重绘的新行；且 POSTDISPLAY 是 ZLE 特殊变量，TRAPINT（非 widget 上下文）中只读无法修改。解决方案：`zle-line-init` 时 `stty intr undef` 让 `^C` 作为普通按键进入 ZLE，绑定 `zsh-as-ctrl-c` widget（清空 POSTDISPLAY/高亮/状态 + `zle .send-break` 中断当前行）；`zle-line-finish` / `zshexit` 恢复 `stty intr '^C'` 保证命令执行期间 `^C` 走 SIGINT。

同理，回车（`accept-line`）走 line_submit action（`ZSH_AS_LINE_SUBMIT_WIDGETS`，含 accept-line / accept-and-hold / accept-line-and-down-history）：清空 POSTDISPLAY 变量不等于擦除屏幕字符——accept-line 直接换行会让建议灰字滞留。line_submit 在调用 original widget 前若原 POSTDISPLAY 非空则 `zle -R` 强制重绘擦除残留（仅作用于换行类 widget，回车后立即进入新 ZLE 周期，重绘无副作用；不影响 modify 通用路径的 suggestion 渲染），使「不接受建议直接回车」与「Ctrl+C」屏幕表现一致。
