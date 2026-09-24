# awake

合盖不休眠扩展。核心是系统全局睡眠开关 `pmset -a disablesleep`——这是全局唯一能实现「合盖 + 无外接显示器 + 电池供电」不休眠的杠杆（`IOPMAssertion` 压不住 clamshell 睡眠路径，虚拟显示器方案又被 macOS 的 clamshell 电源前置条件挡死，已移除）。开关语义为全局睡眠禁用：开盖闲置时显示照常息屏、系统不睡；合盖时持续运行。

## 机制

三层结构：

- `platform/sleep.rs::spawn_sleep_watchdog`：经 osascript 管理员授权启动一个 root 后台 sh 循环。循环每 2 秒轮询 flag 文件（`ext_data_dir/extensions/awake/sleep-watchdog-<pid>.flag`，按 app pid 命名防快速重启时旧 watchdog 退出清理误删新实例的 flag）与 app pid：flag 出现 → 上翻 `disablesleep 1`、消失 → 回落 0（各仅写一次，边沿触发）；app 退出/崩溃 → 恢复默认睡眠并自清 flag。
- **熄屏巡检**（awake setup 的 2s tick）：`disablesleep` 挡住合盖睡眠后系统不会自动关内屏（无外接时 macOS 走的是睡眠路径而非 clamshell 关屏路径，背光常亮），须主动熄屏——合盖 + 无外接的边沿立即 `pmset displaysleepnow`（无需 root、对已灭屏幂等），停留期每 5s 节流补熄（面板被通知等重新点亮）。有外接屏时不熄（clamshell 外接显示是正常用法，该命令是全屏级会把外接屏一起黑掉）。合盖检测读 IORegistry `IOPMrootDomain` 的 `AppleClamshellState`（偶发读取失败以闭锁防抖，None 不翻转既有判定）；外接判定走 CG 活动显示列表的非内置屏。两者均为微秒级 syscall 无子进程。开盖瞬间与熄屏动作存在毫秒级竞态窗口（读 lid 后恰开盖），后果为开盖首帧被熄一次、任意输入唤醒，接受（Keepresso 同构方案同样存在）。
- `extensions/awake/native/mod.rs`：意图状态（`AtomicBool`）+ 授权一次性守卫（`helper_started`，本 app 运行期首次开启弹一次密码，之后全靠 flag 文件零弹窗）。

## 关键决策

- **flag 文件而非每次直写**：授权一次后开关即纯文件操作；边沿触发避免与用户手工 `sudo pmset` 持续互相覆盖（单次覆盖可见、可接受）。
- **watchdog 绑定 app 进程（pid 监视）**：崩溃安全的最小机制——app 死则系统自动恢复正常睡眠，无需落盘恢复债务、无需守护进程。代价是语义为「跟随 app 生命周期」：app 退出即失效，重启后由 config watch（immediate）自动重新持有，每次启动重新授权一次。若未来需要跨 app 生死保持，升级路径是 SMAppService LaunchDaemon + 按连接计数 hold（Keepresso 模式），当前不做。
- **授权失败回写纠偏**：取消/失败时删 flag，前端 config watch 的 catch 将 `enabled` 回写 false，防「配置说开、系统实际没开」漂移到下次启动反复弹窗。
- **启动清理**：setup 同步删 stale flag（断电/被杀场景残留；正常路径 watchdog 自愈）与旧版虚拟显示器 binary。
- **授权并发守卫**（`engaging` AtomicBool CAS）：授权弹窗模态阻塞期间二次开启会被拒绝，防 osascript 授权对话框叠加。
- **电池护栏**：`disablesleep` 会压住系统的低电量睡眠路径（Keepresso 实证：合盖 Mac 一路跑过截止线），60s 低频巡检 `pmset -g batt`，放电中低于 20% 即解除持有（删 flag，watchdog 回落），系统随即入睡；读取失败/插电一律 no-op。一次性解除无自动恢复（滞回随之不需要），重新开启由用户决定。
- **Rust 侧关闭的 config 回写**：菜单栏开关与电池护栏解除都经 `awake-enabled` 事件由 `config.ts` 模块级 listener 回写 `enabled=false`（不依赖 View 挂载），防重启后 watch immediate 误重新持有。
- **菜单栏快捷开关**：config `menubarToggleVisible`（默认 false）经 watch 同步 Rust `set_awake_menubar_visible`；开启后菜单段常驻（不随 enabled 显隐），CheckItem 勾选态反映 enabled、点击按当前状态取反。开启路径走授权弹窗，取消/进行中静默（勾选态不变，重试即可）。

## 远程会话

合盖后无任何显示器时，macOS 为远程会话（Screen Sharing/VNC）合成 1920×1080 非 Retina framebuffer，功能完整、文字略虚。需要高清晰度时用系统自带虚拟显示器（系统设置 → 显示器 → 高级），不自建——虚拟屏会被 CoreGraphics 计入外接屏，若与熄屏策略并存会互相干扰。
