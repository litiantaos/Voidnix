# finder-ext（访达工具）

全局快捷键唤起操作面板，对当前访达选区 / 窗口执行轻量动作。**不使用 FinderSync `.appex`**，无右键注入、无 pluginkit、无独立打包步骤；iCloud 等路径与本地一致（走 Finder 应用脚本接口）。

## 交互

- 默认快捷键 `Option+F`（代码标识符 `Alt+F`；`globalShortcuts` id=`finder-ext`，可在面板内改；dev 构建按框架规则叠加 Shift）
- 再按一次同快捷键：已在本模块则隐藏窗口（`makeToggleHandler`）
- 面板：操作列表 + 启动快捷键配置；↑↓ 选中、回车执行；成功后隐藏窗口（有 toast 则短延迟）

## 动作

统一命令 `finder_run_action`（`CMD.finderRunAction`），`action`：

- `copy_path`：选中项路径写入剪贴板（多行）；无选中则用当前窗口目标目录
- `open_terminal`：在选中项所在目录（或目标目录）打开 Terminal.app
- `new_file`：`BaseDialog` 单输入——默认 `Untitled.txt`，打开时选中扩展名前的文件名主体 → 创建 → 访达中选中
- `toggle_hidden`：注入 `Cmd+Shift+.`（与系统一致、**不重启访达**）；需辅助功能；文案固定「切换隐藏文件」（系统无稳定可读显示态，不做两态文案）

路径均经 `platform/path_guard`。

## 上下文入口（视频 / 图片处理）

当选中视频或图片文件时，操作列表顶部动态出现「视频处理」/「图片处理」项（副标题显示文件名；多选视频显示 `{n} 个视频`），回车即跳转对应扩展并带入路径。

- 探测：面板 `onActivated` 调 `finder_selected_paths` 读访达选区，同一 `detectSelection` 分别按视频 / 图片扩展名白名单过滤（视频收集全部命中供批量处理，图片取首个命中）；快捷键重入（KeepAlive 下 `onActivated` 不触发）由 `reactivateTick` 信号驱动重新探测
- 视频白名单：与 video 扩展 `VIDEO_EXTENSIONS` 基本一致，**去除 `.ts`**（与 TypeScript 源码歧义）；此处仅作 UI 入口提示，真正处理以 video 扩展 ffprobe 为准
- 图片白名单：`IMAGE_EXT_SET`（png/jpg/jpeg/heic/heif/webp/tiff/tif/bmp/gif），镜像自 image 扩展 `IMAGE_EXTENSIONS`（新增格式双向同步）
- 跨扩展通信：`emit('video-pending-input-path', paths[])`（数组，多选区全量）/ `emit('image-pending-input-path', path)` + `setActiveExtension`；对应扩展 setup 监听事件写入各自 `pendingInputPaths` / `pendingInputPath`，View watch 后加载（与 screenshot→translate 同一事件总线模式）
- 访达非前台 / 权限缺失 / 无视频或图片选中 → 入口不出现（静默，不报错）

## 命令

- `finder_run_action`（`CMD.finderRunAction`）：执行动作（见上）
- `finder_selected_paths`（`CMD.finderSelectedPaths`）：返回访达当前选中的文件路径（仅前台为访达时）

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
- **无落盘扩展 config**；快捷键覆盖走框架 `settings.shortcutOverrides`

## 文件

```
extensions/finder-ext/
├── index.ts          # defineExtension + globalShortcuts
├── locales.ts        # 扩展文案（i18n 注册）
├── shortcuts.ts      # 快捷键 id/默认值 + 动作列表
├── View.vue          # BaseSettingsList（操作 + 快捷键）+ 选区视频 / 图片探测
└── native/mod.rs     # finder_run_action + finder_selected_paths + JXA 上下文 + 动作实现
```

## 已知限制

- 终端固定 Terminal.app（不读用户默认终端）
- 拷贝路径 / 终端 / 新建要求访达 frontmost；`toggle_hidden` 除外
- 无访达窗口时 `new_file` / 无选中且无 target 的 `open_terminal` 会失败并提示
