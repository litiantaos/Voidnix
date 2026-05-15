# 手工验收 Checklist — webkit-presentation-tuning

本文档列出无法通过自动化测试覆盖的视觉/资源类验收项。
每项均提供"执行步骤"、"判定标准"、"对照组"三栏。
Release 前由维护者逐项勾选。

---

## 验收项 1：唤起首帧无 Stale_Frame / Apparent_White_Gap（Req 1.3, 1.4）

| 字段 | 内容 |
|---|---|
| **执行步骤** | 1. `bun run tauri dev` 启动，使用一段时间后按 Esc 隐藏窗口。<br>2. 等待 3 秒（确保 WebKit 进入节流状态）。<br>3. 按全局快捷键唤起，同时用 QuickTime 或 `screencapture -V` 录屏（60fps）。<br>4. 逐帧检查唤起瞬间的画面。 |
| **判定标准** | 窗口出现的第一帧即为当前应有的 UI（搜索框 + 空结果列表），不出现：<br>• 上一次会话的残留内容（Stale_Frame）<br>• 白色或透明矩形条（Apparent_White_Gap）<br>• 空白闪烁（alpha 从 0 到 1 的过渡不可见） |
| **对照组** | `VOIDNIX_DISABLE_WEBKIT_TUNING=1 bun run tauri dev` 下重复上述步骤，应可观察到偶发的陈旧内容或白底闪烁，以确认驯化逻辑确实生效。 |

---

## 验收项 2：列表 ↔ 扩展面板 ↔ 设置 尺寸切换无白边（Req 3.4）

| 字段 | 内容 |
|---|---|
| **执行步骤** | 1. 唤起主窗口，进入任意模块（如剪贴板）。<br>2. 打开设置面板，再关闭，再打开，快速来回切换 10 次。<br>3. 录屏并逐帧检查窗口圆角内侧。 |
| **判定标准** | 每次尺寸切换过程中及切换完成后，窗口圆角内侧不出现白色或透明矩形条（Apparent_White_Gap）。动画过程连续平滑，无停帧。 |
| **对照组** | `VOIDNIX_DISABLE_WEBKIT_TUNING=1` 下重复，应可观察到切换时的白边或抖动。 |

---

## 验收项 3：含 emoji 的视图首次渲染无停顿（Req 4.3）

| 字段 | 内容 |
|---|---|
| **执行步骤** | 1. 冷启动 Voidnix（确保 emoji 字体未被预热）。<br>2. 唤起主窗口，立即切换到剪贴板模块（需要有含 emoji 的剪贴板条目）。<br>3. 录屏并观察 emoji 字符的首次渲染。 |
| **判定标准** | emoji 字符在其所在帧内完整渲染，不出现先显示豆腐块/方框、后续帧才补绘字形的现象。 |
| **对照组** | `VOIDNIX_DISABLE_WEBKIT_TUNING=1` 下重复，冷启动后首次 emoji 可能出现短暂的字体回退停顿。 |

---

## 验收项 4：macOS 13/14/15/26 各版本启动不崩溃（Req 5.1）

| 字段 | 内容 |
|---|---|
| **执行步骤** | 1. 在各目标 macOS 版本上安装 release 构建（`./deploy.sh` 产物）。<br>2. 启动 Voidnix，按全局快捷键唤起 3 次，隐藏 3 次。<br>3. 检查 Console.app 中是否有 crash report 或 `[webkit_tuning]` 错误日志。 |
| **判定标准** | 进程正常启动，主窗口可被全局快捷键唤出和隐藏，Console.app 中无 crash report，无 `[webkit_tuning] caught Obj-C exception` 以外的异常日志（该日志表示驯化逻辑已正确捕获并回退）。 |
| **对照组** | 同一版本下 `VOIDNIX_DISABLE_WEBKIT_TUNING=1` 启动，确认基础功能正常，以排除非驯化逻辑引起的问题。 |

---

## 验收项 5：后台无持续 CPU 轮询（Req 6.1）

| 字段 | 内容 |
|---|---|
| **执行步骤** | 1. 启动 Voidnix，唤起后立即隐藏，等待 60 秒。<br>2. 打开 Activity Monitor → CPU 标签，观察 Voidnix 进程的 CPU 占用。<br>3. 采样 60 秒，记录平均 CPU 占用率。 |
| **判定标准** | 隐藏状态下（前端无活跃定时器），Voidnix 主进程的 CPU 平均占用率 ≤ 0.5%（60 秒采样窗口）。Activity Monitor 底部的 Memory Pressure 图表保持绿色。 |
| **对照组** | `VOIDNIX_DISABLE_WEBKIT_TUNING=1` 下重复，基线 CPU 占用应相近（驯化逻辑不引入额外轮询）。 |

---

## 验收项 6：常驻内存增量 ≤ 10MB（Req 6.2）

| 字段 | 内容 |
|---|---|
| **执行步骤** | 1. 启动 Voidnix，唤起后隐藏，等待 60 秒稳定。<br>2. 在 Activity Monitor 中记录 Voidnix 主进程的 Memory（RSS）。<br>3. 用 `VOIDNIX_DISABLE_WEBKIT_TUNING=1` 重复上述步骤，记录基线 RSS。<br>4. 计算差值。 |
| **判定标准** | 驯化启用时的 RSS 相对基线增量 ≤ 10MB（启动后稳定 60 秒采样）。 |
| **对照组** | 见步骤 3。 |

---

## 验收项 7：RUST_LOG=webkit_tuning=debug 输出 5 条 component= 日志（Req 7.3）

| 字段 | 内容 |
|---|---|
| **执行步骤** | 1. `RUST_LOG=webkit_tuning=debug bun run tauri dev` 启动。<br>2. 观察终端 stderr 输出。 |
| **判定标准** | 启动后 stderr 出现恰好 5 条 `component=` 日志，分别对应：<br>• `component=Throttling_Suppressor status=启用`<br>• `component=Webview_Frame_Pin status=启用`<br>• `component=Frame_Animator status=启用`<br>• `component=Emoji_Warmer status=启用`（约 500ms 后）<br>• `component=Presentation_Coordinator status=启用`<br>每次唤起/隐藏后出现 `event=show/hide steps=[...]` 日志。 |
| **对照组** | `VOIDNIX_DISABLE_WEBKIT_TUNING=1 RUST_LOG=webkit_tuning=debug bun run tauri dev` 下，stderr 只出现 `component=Tuning_Toggle status=已禁用` 一条日志。 |

---

## 勾选记录

| 验收项 | macOS 版本 | 日期 | 结果 | 备注 |
|---|---|---|---|---|
| 1. 唤起首帧无闪烁 | | | ☐ 通过 / ☐ 失败 | |
| 2. 尺寸切换无白边 | | | ☐ 通过 / ☐ 失败 | |
| 3. emoji 首次渲染 | | | ☐ 通过 / ☐ 失败 | |
| 4. macOS 13 启动 | macOS 13 | | ☐ 通过 / ☐ 失败 | |
| 4. macOS 14 启动 | macOS 14 | | ☐ 通过 / ☐ 失败 | |
| 4. macOS 15 启动 | macOS 15 | | ☐ 通过 / ☐ 失败 | |
| 4. macOS 26 启动 | macOS 26 | | ☐ 通过 / ☐ 失败 | |
| 5. 后台 CPU ≤0.5% | | | ☐ 通过 / ☐ 失败 | |
| 6. RSS 增量 ≤10MB | | | ☐ 通过 / ☐ 失败 | |
| 7. 日志输出正确 | | | ☐ 通过 / ☐ 失败 | |
