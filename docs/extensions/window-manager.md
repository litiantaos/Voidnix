# window-manager

窗口布局与分屏。鼠标移至**任意屏**顶部中心触发区滑出 snap 面板，点击分区将前台窗口贴到对应布局。

## 能力

- 开关：`config.enabled`（watch immediate → `set_window_manager_enabled`）
- 自定义尺寸：`customWidth` / `customHeight`（floor/cap 权威在 `native/mod.rs`，CI `check:wm-bounds` 镜像 TS `BOUNDS`）
- 布局：四角 / 上下半 / 左右半 / 全屏 / 居中 / 自定义居中尺寸
- **多屏**：布局相对窗口所在屏；多屏时面板末组提供 prev/next display（相对几何映射到邻屏）

## 多屏坐标

两套坐标系严格分离，禁止混用：

- **Cocoa**（`NSScreen.frame`，左下原点、y 向上）：snap 面板定位、鼠标命中（与 `NSEvent.mouseLocation` 一致）
- **AX**（primary 左上原点、y 向下）：`AXPosition` / `AXSize` / System Events position

翻转锚点是 **primary**（frame 原点近 (0,0) 的菜单栏屏），**不是** `NSScreen.mainScreen`（随键盘焦点漂移）。`ax_y = primary_max_y − cocoa_y − h`。

**布局区**（`layout_*`）以 `visibleFrame` 为底，但全局只认一屏为底 Dock 宿主：

- 底 Dock 宿主判定：`bottom_inset` 最大，并列时 primary 优先
- 其它屏若被系统误扣 Dock 高，则把底边拉回 `frame` 底（修副屏下半/左下/右下贴底留白）
- 菜单栏顶 inset 与侧边 Dock 仍跟 visible；结果始终夹进 `frame`

**写入顺序**：AX / AppleScript 均 `size → position → size`：

- macOS 按当前屏钳制尺寸；先 position 再 size 时，下半/左下/右下会以旧高度短暂跨出副屏底边（竖排副屏时直接压到主屏）
- 目标矩形再夹进 `layout_*`

**EnhancedUI guard**（`apply_ax_layout` 入口）：写窗口前若目标 app 的 `AXEnhancedUserInterface == true` 则关 false，写完恢复。Chromium 系（Chrome 等）默认开启该属性，使 AX 写窗口触发异步动画（~250ms）且 `size → position` 连续写入时 position 被吞、size 漂移；关闭后写入瞬时精确（0ms 动画）。无此属性的应用（大多数）为 no-op（Rectangle 同款策略）。

**前置检查**（`apply_ax_layout` 入口，AX 路径）：

- **全屏窗口跳过**：读 `AXFullScreenButton` 子元素 `AXSubrole`，`AXZoomButton` = 全屏中。原生全屏窗口 AX 写 frame 行为不可预测，直接 no-op
- **固定尺寸窗口**：`AXUIElementIsAttributeSettable(AXSize)` 检测。不可缩放时只写 position（区域左上角定位），尺寸保持不变。跨屏迁移只按比例移动位置

窗口归属屏：读 AX 位置 + 尺寸，用**中心点**命中 `ax_frame_*`；重叠时取面积更小屏。AppleScript 回退同样先取窗口几何再选屏。

## 架构

- `native/mod.rs`：扩展入口、命令、`ScreenInfo`、snap-panel 进出场动画
- `native/platform.rs`：枚屏、布局计算、AX / AppleScript 应用
- `native/window_snap.rs`：全局/本地 mouseMoved、触发区、面板 show/hide
- `windows/SnapPanel.vue`：分区 UI；`__snapPanelData.screens > 1` 时显示跨屏组
- `View.vue` / `config.ts`：设置页 + defineConfig

需**辅助功能权限**（AX）；未授权时走 System Events AppleScript。

## 命令

- `set_frontmost_window_layout` — layout + 可选 custom 尺寸 + prev_pid
- `set_window_manager_enabled` / `set_snap_size`
- `show_snap_panel` / `hide_snap_panel` — 仅动画与焦点归还，定位由 `window_snap::show_panel` 完成

## 面板尺寸

`p-3×2 + n×w-14 + (n−1)×gap-3`，高固定 80。单屏 n=5（352），多屏 n=6（420）。Rust `panel_dimensions()` 与 Vue 组数同步。
