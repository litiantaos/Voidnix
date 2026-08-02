# homebrew（包管理）

可视化 Homebrew 面板：已安装包列表、可升级检测、一键更新升级、服务启停、包详情（依赖 / 被依赖）与卸载。纯查询 + 流式执行，无配置持久化。

## 交互

- 主视图（`mainView`）：Homebrew 状态行（版本 / formula 数 / cask 数 / 可升级数）+ 服务行（启停 / 重启）+ 包列表（formula 在前、cask 在后，各按名称排序）。搜索框实时过滤包名
- 子视图 `detail`（`subviews.detail`，标题「包详情」）：目标包摘要 + 依赖 + 被依赖，依赖 / 被依赖项可回车递归进入详情
- 一键更新：状态行右侧「更新」按钮 → 流式执行 `update → upgrade → cleanup → autoremove`，完成后刷新状态
- 卸载：详情页右侧「卸载」→ `showConfirm` 展示被依赖数 + 孤立依赖自动清理提示 → 流式执行 `uninstall → autoremove`

## 命令

- `brew_status`（`CMD.brewStatus`）：版本号 + 全部已安装包（formula / cask，含当前版本、最新版本、描述）+ `has_update`
- `brew_services`（`CMD.brewServices`）：服务列表（name + status）
- `brew_info`（`CMD.brewInfo`，`name`）：包详情（desc + 依赖 + 被依赖，均含版本与描述）
- `brew_run`（`CMD.brewRun`，`operation` / `target?` / `onEvent`）：流式执行 brew 子命令，stdout + stderr 逐行经 `Channel<BrewEvent>` 回传

`brew_run` 的 `operation` 取值：`update_upgrade` / `uninstall` / `autoremove` / `services_start` / `services_stop` / `services_restart`。内部按 operation 展开为有序步骤（如 `update_upgrade` = 四步），逐步流式执行，任一步失败即终止。

## 实现要点

- **brew 路径探测**：硬探测 `/opt/homebrew/bin/brew`（Apple Silicon）→ `/usr/local/bin/brew`（Intel），不依赖 GUI 进程 PATH（GUI 启动的进程 PATH 可能不含 brew bin 目录）
- **PATH 补全**：`ensure_brew_path` 在调用前将 Homebrew bin/sbin 及系统路径拼入 PATH，保证 brew 子命令（如 `brew update` 触发的 git）能找到依赖工具
- **并发采集**：`brew_status` 用 `tokio::join!` 并发拉取版本号、formula 列表、cask 列表、过期检测，再并发批量拉取包摘要（`brew info --json=v2` 读本地缓存，不联网），最后组装
- **过期检测**：`brew outdated --json=v2` → name → (installed, current) 映射，组装时填充 `new_version`（空 = 已是最新）
- **摘要解析**：`parse_summaries` 统一处理 formulae（`name` + `versions.stable`）与 casks（`token` + `version`）两段 JSON
- **流式执行**：`run_brew_step` spawn 子进程，stdout + stderr 各起一个 reader task，经 mpsc 合流后逐行 `on_event.send`；reader 结束（管道 EOF）= 子进程已退出，再 `wait` 取退出码
- **kill_on_drop**：子进程带 `kill_on_drop(true)`，Channel 断开或任务取消时自动回收

## 子视图数据传递

详情页目标（name / kind / version / desc）经 `sessionStorage['homebrew:detail']` 从主视图传入。DetailView `onActivated` 读取并清空搜索框，回车递归进入依赖项时覆写同一 key 后重新 `fetchInfo`。

## 文件

```
extensions/homebrew/
├── index.ts          # defineExtension（mainView + subviews.detail）
├── View.vue          # 状态行 + 服务行 + 包列表（过滤 + 执行分派）
├── DetailView.vue    # 包详情（依赖 / 被依赖 / 卸载）
└── native/mod.rs     # brew_status / brew_services / brew_info / brew_run + JSON 解析 + 流式执行
```

## 已知限制

- 无配置持久化（纯查询 + 执行，无 `config.ts`）
- 卸载依赖 `brew info` / `brew uses` 判断，formula 与 cask 的依赖关系粒度以 brew 自身输出为准
- 服务管理仅 start / stop / restart 三态，不展示日志
