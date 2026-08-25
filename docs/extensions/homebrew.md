# homebrew（包管理）

可视化 Homebrew 面板：已安装包列表、可升级检测、一键更新升级、服务启停、包详情（依赖 / 被依赖）与卸载。纯查询 + 流式执行，无配置持久化。

## 交互

- 主视图（`mainView`）：Homebrew 状态行（版本 / formula 数 / cask 数 / 可升级数）+ 服务行（启停 / 重启）+ 包列表（formula 在前、cask 在后，各按名称排序）。搜索框实时过滤包名
- 进入视图即拉状态（KeepAlive 缓存下重进走 `onActivated` 重新拉取）；api 元数据陈旧（>24h）时后台 `brew update` 刷新，「更新」按钮位先以旋转禁用态显示「拉取更新」，完成后自动重拉刷新可升级数
- 子视图 `detail`（`subviews.detail`，标题「包详情」）：目标包摘要 + 依赖 + 被依赖，依赖 / 被依赖项可回车递归进入详情
- 一键更新：状态行右侧「更新」按钮 → 流式执行 `update → upgrade → cleanup → autoremove`，完成后刷新状态；主视图首项（状态行）回车/双击亦触发（有更新且非运行中时）
- 卸载：详情页右侧「卸载」→ `showConfirm` 展示被依赖数 + 孤立依赖自动清理提示 → 流式执行 `uninstall → autoremove`；详情页首项回车直接进入卸载确认

## 命令

- `brew_status`（`CMD.brewStatus`）：版本号 + 全部已安装包（formula / cask，含当前版本、最新版本、描述）+ `has_update` + `refreshing`（后台元数据刷新在途）
- `brew_services`（`CMD.brewServices`）：服务列表（name + status）
- `brew_info`（`CMD.brewInfo`，`name`）：包详情（desc + 依赖 + 被依赖，均含版本与描述）
- `brew_run_state`（`CMD.brewRunState`）：查询当前 `brew_run` 运行态（`{ operation, step } | null`），不调 brew，零开销
- `brew_run`（`CMD.brewRun`，`operation` / `target?` / `onEvent`）：流式执行 brew 子命令，stdout + stderr 逐行经 `Channel<BrewEvent>` 回传

`brew_run` 的 `operation` 取值：`update_upgrade` / `uninstall` / `autoremove` / `services_start` / `services_stop` / `services_restart`。内部按 operation 展开为有序步骤（如 `update_upgrade` = 四步），逐步流式执行，任一步失败即终止。

## 实现要点

- **brew 路径探测**：硬探测 `/opt/homebrew/bin/brew`（Apple Silicon）→ `/usr/local/bin/brew`（Intel），不依赖 GUI 进程 PATH（GUI 启动的进程 PATH 可能不含 brew bin 目录）
- **PATH 补全**：`ensure_brew_path` 在调用前将 Homebrew bin/sbin 及系统路径拼入 PATH，保证 brew 子命令（如 `brew update` 触发的 git）能找到依赖工具
- **禁止隐式自动更新**：所有 brew 命令经 `brew_command` 助手统一注入 `HOMEBREW_NO_AUTO_UPDATE=1`，消除 brew 隐式联网拉取元数据的等待（实测首次加载从 ~40s 降至 ~1s）；用户点「更新」时 `brew update` 仍是显式全量更新
- **后台元数据刷新**：`NO_AUTO_UPDATE` 的代价是 `brew outdated` 只对比本地 api 缓存，元数据陈旧时查不到新版本。`brew_status` 检测 api 元数据 mtime 超 24h（brew 自身 auto-update 同款节律与判定源；brew 6 起 `internal/packages.<arch>.jws.json`，brew 4/5 为根目录 `formula.jws.json`，取最新值）时后台 spawn `brew update`（120s 超时 + `kill_on_drop`），快路径立即返回缓存态 + `refreshing: true`。刷新复用 `RunGuard` 占用 `BREW_RUNNING`（`brew_run_state` 可查、`brew_run` 拒并发），Drop 时 emit `brew-run-done` 驱动前端重拉——零新增事件；mtime 天然随任何来源的 `brew update`（含终端、一键更新）刷新，24h 判定无需自维护时间戳。失败重试受进程内 10min 冷却约束——否则完成事件的重拉再调 `brew_status` 时 mtime 未变（上次 update 失败），将形成 spawn → 失败 → 重拉 → 再 spawn 的无限环（实测 `brew update` 即使 `Already up-to-date` 也触碰 api 文件 mtime，成功路径天然被 24h 节流覆盖）
- **并发采集**：`brew_status` 用 `tokio::join!` 并发拉取版本号、全部已安装包（`brew info --json=v2 --installed` 一次取 formulae+casks 的名称/描述/已安装版本）、过期检测，共 3 次 brew 进程
- **过期检测**：`brew outdated --json=v2` → name → (installed, current) 映射，组装时填充 `new_version`（空 = 已是最新）。不用 `brew info` 的版本对比替代——revision 后缀（如 `1.5.4_1`）会导致误报
- **已安装版本**：`parse_installed` 从 `brew info --json=v2 --installed` 解析——formulae 读 `installed[0].version`（数组），casks 读 `installed`（字符串）
- **流式执行**：`run_brew_step` spawn 子进程，stdout + stderr 各起一个 reader task，经 mpsc 合流后逐行 `on_event.send`；reader 结束（管道 EOF）= 子进程已退出，再 `wait` 取退出码
- **kill_on_drop**：子进程带 `kill_on_drop(true)`，Channel 断开或任务取消时自动回收
- **运行态持久化**：`BREW_RUNNING`（`LazyLock<Mutex<Option<BrewRunState>>>`）跨组件生命周期持久化。`brew_run` 经 RAII guard（`RunGuard`）占位 + 逐步 `set_step`，Drop 时自动清空 + emit `brew-run-done` 事件。guard 拒绝并发调用（返回错误「已有 Homebrew 操作正在运行」）。前端 `onActivated` 先查 `brew_run_state`：有残留操作则恢复运行态（按钮旋转禁用显示当前步骤）并照常拉数据渲染列表——`brew_status` 三条只读命令与运行中 brew 操作并发安全，不阻断为加载态等操作结束，完成经 `brew-run-done` 统一重拉；无则正常加载。查询响应与完成事件的到达竞态（两条投递通道无顺序保证）经 `doneSeen` 标记丢弃过期 Some，防恢复已结束的操作态卡死运行态。状态拉取带 seq 竞态守卫（恢复态拉取与完成重拉并发时仅最新一轮落盘）

## 子视图数据传递

详情页目标（name / kind / version / desc）经 `sessionStorage['homebrew:detail']` 从主视图传入。DetailView `onActivated` 读取并清空搜索框，回车递归进入依赖项时覆写同一 key 后重新 `fetchInfo`。

## 文件

```
extensions/homebrew/
├── index.ts          # defineExtension（mainView + subviews.detail）
├── View.vue          # 状态行 + 服务行 + 包列表（过滤 + 执行分派）
├── DetailView.vue    # 包详情（依赖 / 被依赖 / 卸载）
├── locales.ts        # 扩展文案注册（import side-effect 进 i18n）
└── native/mod.rs     # brew_status / brew_services / brew_info / brew_run_state / brew_run + JSON 解析 + 流式执行 + RunGuard
```

## 已知限制

- 无配置持久化（纯查询 + 执行，无 `config.ts`）
- 卸载依赖 `brew info` / `brew uses` 判断，formula 与 cask 的依赖关系粒度以 brew 自身输出为准
- 服务管理仅 start / stop / restart 三态，不展示日志
