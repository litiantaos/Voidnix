# finder-ext（访达工具）

全局快捷键唤起操作面板，对当前访达选区 / 窗口执行轻量动作。**不使用 FinderSync `.appex`**，无右键注入、无 pluginkit、无独立打包步骤；iCloud 等路径与本地一致（走 Finder 应用脚本接口）。

## 交互

- 默认快捷键 `Option+F`（代码标识符 `Alt+F`；`globalShortcuts` id=`finder-ext`，可在面板内改；dev 构建按框架规则叠加 Shift）
- 再按一次同快捷键：已在本模块则隐藏窗口（`makeToggleHandler`）
- **两种进入模式**（按进入方式区分，不看访达前台状态）：
  - **访达快捷键进入**（上下文面板）：用 App 打开候选组（置顶，见下；候选为空时隐藏）+ 选区视频/图片入口 + 操作列表 + 启动快捷键配置；↑↓ 选中、回车执行；成功后隐藏窗口（有 toast 则短延迟）
  - **应用界面进入**（浏览模式：搜索 / 工具列表 / 扩展切换）：无访达上下文，不探测选区、无候选组，显示全量操作目录（用 App 打开 → 视频处理 → 图片处理 → 其余动作，序与上下文面板一致）+ 启动快捷键配置；回车任意操作仅 toast「该操作仅在访达中生效」，不执行不隐藏
- 模式判定：`entryViaShortcut`（makeToggleHandler 回调同步置位，先于 View 挂载 / onActivated 到达）+ `reactivateTick`（快捷键重入，KeepAlive 下 onActivated 不触发，tick watch 恒切回上下文模式）。标记消费即复位、无跨次残留：挂载 / 重激活由 `onActivated` 消费；已激活态重入（窗口隐藏后快捷键再呼出，无 onActivated 跟进——窗口隐藏不反激活，仅扩展切换会）由 tick watch 按 KeepAlive 激活态（onActivated / onDeactivated 配对维护）消费，否则残留 true 会让下次应用界面进入误判为上下文面板

## 动作

统一命令 `finder_run_action`（`CMD.finderRunAction`），`action`（面板目录见 `shortcuts.ts::FINDER_CATALOG` 单一数据源——finder_run_action 动作与选区媒体入口统一编排、序即面板正式序；上下文模式过滤 `open_with`——由候选组承载、媒体入口仅有选区时出现；浏览模式全量显示）：

- `copy_path`：选中项路径写入剪贴板（多行）；无选中则用当前窗口目标目录
- `open_with`：用指定应用打开选中项（`app_path` 传 .app 路径）；多选全部打开；无选中回退当前窗口目标目录
- `open_terminal`：在选中项所在目录（或目标目录）打开 Terminal.app
- `new_file`：`BaseDialog` 单输入——默认 `Untitled.txt`，打开时选中扩展名前的文件名主体 → 创建 → 访达中选中
- `toggle_hidden`：注入 `Cmd+Shift+.`（与系统一致、**不重启访达**）；需辅助功能；文案固定「切换隐藏文件」（系统无稳定可读显示态，不做两态文案）

路径均经 `platform/path_guard`（`open_with` 的应用路径除外，见实现要点）。

## 上下文入口（视频 / 图片处理）

当选中视频或图片文件时，操作列表顶部动态出现「视频处理」/「图片处理」项（副标题显示文件名；多选显示 `{n} 个视频` / `{n} 张图片`），回车即跳转对应扩展并带入路径。

- 探测：面板 `onActivated` 调 `finder_selected_paths` 读访达选区，同一 `detectSelection` 分别按视频 / 图片扩展名白名单过滤（均收集全部命中供批量处理；图片多张由 image 扩展自动进拼接模式）；快捷键重入（KeepAlive 下 `onActivated` 不触发）由 `reactivateTick` 信号驱动重新探测
- 视频白名单：与 video 扩展 `VIDEO_EXTENSIONS` 基本一致，**去除 `.ts`**（与 TypeScript 源码歧义）；此处仅作 UI 入口提示，真正处理以 video 扩展 ffprobe 为准
- 图片白名单：`IMAGE_EXT_SET`（png/jpg/jpeg/heic/heif/webp/tiff/tif/bmp/gif），镜像自 image 扩展 `IMAGE_EXTENSIONS`（新增格式双向同步）
- 跨扩展通信：`window.dispatchEvent(new CustomEvent('video-pending-input-path', { detail: paths[] }))` / `window.dispatchEvent(new CustomEvent('image-pending-input-path', { detail: paths[] }))`（均数组，多选区全量）+ `setActiveExtension`；对应扩展 setup 监听事件写入各自 `pendingInputPaths`，View watch（immediate）后加载（与 screenshot→translate 同一模式；image 按张数分流——单张直达 removeBg、多张自动切拼接）。**同步投递先于跳转首帧**：经 IPC 往返会晚一拍，期间目标列表形状未定型，快速 ↓+Enter 会误中「选择文件」行弹系统文件选择器
- 访达非前台 / 权限缺失 / 无视频或图片选中 → 入口不出现（静默，不报错）
- 浏览模式（应用界面进入）：入口以目录行形式恒显（无副标题、不探测选区），回车仅提示「该操作仅在访达中生效」（不跳转扩展）

## 用 App 打开

场景（按频次）：

1. **用编辑器打开文件夹**（核心）：访达选中项目文件夹 → 用 VS Code/Zed 等打开。访达右键「打开方式」对文件夹只显示 Finder（菜单层过滤），但 LaunchServices 注册数据里编辑器在列（实测 VS Code/Zed 均注册 folder handler）——直接消费 LS 数据即可拿到编辑器候选
2. **用编辑器/其他应用打开文件**：替代右键 → 打开方式的深层菜单，键盘流直达
3. **多选**：选中多个文件全部交给目标应用（`open -a <app> f1 f2…`，语义同访达「打开方式」多选；候选按首项类型推荐）
4. **无选中**：回退当前访达窗口目标目录（与 copy_path / open_terminal 一致，「在当前目录打开编辑器」）

**候选平铺在面板顶部（「用 App 打开」组），无二级界面**——与主流工具同款做法（访达「打开方式」/ Raycast / Alfred：类型推荐 + 最近使用置顶，不让用户默认浏览全部应用）：

- 候选构成：**MRU 最近使用置顶**（跨类型，上限 3）→ **LaunchServices 类型推荐补足**（偏好序：默认应用在前，`NSWorkspace.URLsForApplicationsToOpenURL`，与访达「打开方式」同源数据）→ 去重后截断至 5 行；候选为空（无选区且无 MRU）时组整体隐藏
- LS 候选与已安装列表 join 不上的（Playwright 缓存浏览器 / 系统卷投影等脏项）静默剔除；Finder 自身（目录默认 handler）Rust 端过滤；LS 返回的 Cryptexes 投影路径（`/System/Volumes/Preboot/Cryptexes/App/System/...`）Rust 端归一化为 `/System/...` 常规路径，否则系统应用候选（TextEdit/Preview 等）join 失败被误剔
- 回车直接执行候选（`open_with`），成功记忆 MRU——高频路径 `Option+F → Enter` 两键直达常用编辑器
- 候选行图标 = base64 应用图标缩小一半（57%，`BaseListItem.icon` 图片形态；fill-mist 圆角外框保留）

数据与记忆：

- 应用列表**复用 search 扩展命令**（`search_apps` 元数据 + `get_app_icons` 图标按 id 合流）：应用枚举与图标提取的唯一实现，缓存已随全局搜索预热；`apps.ts` 持模块级缓存，`app-cache-updated` / `app-icons-updated` 事件整体失效重拉（镜像 search/index.ts 手法）
- 最近使用落盘 `config.json`（`recentApps`，MRU 上限 3）
- 候选行图标 = base64 应用图标：`BaseListItem.icon` 支持 i- 前缀字体类与 base64 图片双形态（与 `ResultIcon` 同优先级语义）

## 命令

- `finder_run_action`（`CMD.finderRunAction`）：执行动作（见上）；`name` 仅 `new_file`、`app_path` 仅 `open_with`
- `finder_selected_paths`（`CMD.finderSelectedPaths`）：返回访达当前选中的文件路径（仅前台为访达时）
- `finder_open_with_apps`（`CMD.finderOpenWithApps`）：返回 LaunchServices 意义上能打开指定路径的应用路径列表（偏好序，已过滤 Finder；供候选推荐）

## 实现要点

- **上下文**：JXA 读 `selection` + `finderWindows[0].target` → JSON
- **frontmost 守卫**：拷贝路径 / 终端 / 新建要求 frontmost 为访达；切换隐藏不依赖 frontmost
- **新建文件**：`BaseDialog` `closeOnConfirm=false` → 异步创建
  - 成功 → 卸窗；失败 → toast 且弹窗保持（不先关再开）
  - 创建后 `selectFile` 选中
- **切换隐藏**（时序关键）：
  - `ensure_accessibility`（窗口仍可见）→ **先 hide 主窗**归还 key → 前置访达并等 frontmost → `platform/input::post_combo("cmd+shift+.", finder_pid)`
  - 先注入再 hide 时面板仍占 key，按键常被吞，表现为需点两次
- **权限**：控制访达（自动化，读选区/目录）；切换隐藏需辅助功能（失败有明确 toast）
- **open_with 应用路径专用校验**（`validate_app_path`：绝对路径 + `.app` 后缀 + 存在）：不经 path_guard——系统内置应用在 `/System/Applications`（path_guard 拦 `/System` 前缀），`open -a` 交 LaunchServices 启动无文件系统写，且路径源自应用枚举缓存而非用户输入；目标路径（选区/目录）仍走 path_guard
- **快捷键覆盖**走框架 `settings.shortcutOverrides`；`recentApps` 落盘 `defineConfig`

## 文件

```
extensions/finder-ext/
├── index.ts          # defineExtension + globalShortcuts + entryViaShortcut / reactivateTick 进入信号
├── locales.ts        # 扩展文案（i18n 注册）
├── shortcuts.ts      # 快捷键 id/默认值 + 面板目录单一数据源 FINDER_CATALOG（动作 + 媒体入口，序即面板正式序）
├── apps.ts           # 候选数据（复用 search 命令的列表缓存 + buildCandidates 纯函数）
├── config.ts         # recentApps（用 App 打开最近使用，MRU 上限 3）
├── View.vue          # 双进入模式（快捷键上下文面板 / 应用界面浏览模式）+ 候选组平铺 + BaseSettingsList（操作 + 快捷键）+ 选区视频 / 图片探测
└── native/mod.rs     # finder_run_action + finder_selected_paths + finder_open_with_apps + JXA 上下文 + 动作实现
```

## 已知限制

- 终端固定 Terminal.app（不读用户默认终端）
- 拷贝路径 / 用 App 打开 / 终端 / 新建要求访达 frontmost；`toggle_hidden` 除外
- 无访达窗口时 `new_file` / 无选中且无 target 的 `open_terminal` / `open_with` 会失败并提示
