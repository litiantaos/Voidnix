# awake

合盖不休眠扩展。核心是系统全局睡眠开关 `pmset -a disablesleep`——这是全局唯一能实现「合盖 + 无外接显示器 + 电池供电」不休眠的杠杆（`IOPMAssertion` 压不住 clamshell 睡眠路径，虚拟显示器方案又被 macOS 的 clamshell 电源前置条件挡死，已移除）。开关语义为全局睡眠禁用：开盖闲置时显示照常息屏、系统不睡；合盖时持续运行。

## 机制

三层结构：

- `platform/sleep.rs::spawn_sleep_watchdog`：经 osascript 管理员授权启动一个 root 后台 sh 循环。循环每 2 秒轮询 flag 文件（`ext_data_dir/extensions/awake/sleep-watchdog-<pid>.flag`，按 app pid 命名防快速重启时旧 watchdog 退出清理误删新实例的 flag）与 app pid：flag 出现 → 上翻 `disablesleep 1` 并进入持续维持（见下）、消失 → 回落 0；app 退出/崩溃 → 恢复默认睡眠并自清 flag。
- **熄屏巡检**（awake setup 的 2s tick）：`disablesleep` 挡住合盖睡眠后系统不会自动关内屏（无外接时 macOS 走的是睡眠路径而非 clamshell 关屏路径，背光常亮），须主动熄屏。合盖检测读 IORegistry `IOPMrootDomain` 的 `AppleClamshellState`（偶发读取失败以闭锁防抖，None 不翻转既有判定）；外接判定走 CG 活动显示列表的非内置屏；两者均为微秒级 syscall 无子进程。两档策略（config `screenPolicy`，默认 `dim`）：
  - `sleep`（显示睡眠）：合盖 + 无外接的边沿立即 `pmset displaysleepnow`（无需 root、对已灭屏幂等），停留期每 5s 节流补熄。省电最优，但显示子系统真睡、屏幕捕获流冻结——第三方远程（如 UU 远程）停摆。
  - `dim`（零亮度）：内置面板背光归零、framebuffer 保持活跃，远程控制可用。亮度经私有 `DisplayServices.framework` 的 `DisplayServicesGet/SetBrightness(display, float)` 读写（Keepresso 生产验证的同款通道；dlsym 解析，display id 取 online→active→进程内缓存——合盖后两个 CG 列表都会摘掉内置屏）。恢复目标在开盖期持续采样（macOS 合盖首拍即归零，活读常为 0），开盖（或开关关闭）时面板仍暗才恢复（外部已调亮的不覆盖）；符号缺失/无内置屏自动退化显示睡眠。零亮度残留时关闭开关会先还亮度再退场（防开盖黑屏）。**本机新背光架构实测**：IORegistry 直写 `AppleARMBacklight`（32/64 位 CFNumber 均同）与 `CoreDisplay_Display_SetUserBrightness` 都只改报告值不驱动实际背光，且读回验证全部假阳性（写入后读回一致仅说明服务层状态变了）——亮度通道的有效性以物理观察为准，诊断日志带写入返回值。
- `extensions/awake/native/mod.rs`：意图状态（`AtomicBool`）+ 授权一次性守卫（`helper_started`，本 app 运行期首次开启弹一次密码，之后全靠 flag 文件零弹窗）。

## 关键决策

- **flag 文件而非每次直写**：授权一次后开关即纯文件操作。watchdog 为**持续维持**而非边沿写——持有期间每周期校验 `pmset -g` 的 `SleepDisabled`（输出为 `SleepDisabled\t\t1`，匹配须桥接 tab），失配即重写。纯边沿写有实测事故：全局设置被外部清零后持有静默失效，系统按 Clamshell Sleep 入睡（远程会话表现为锁屏解锁瞬间离线）。清零源已实验实锤为 UU 远程——每次会话建立时重置非默认电源项（对照实验：置 1 后两次 UU 连接即清零，静置无连接 4 分钟保持 1）；另一实例退出恢复、手动 `sudo pmset` 同为可能源。持续维持使外部清零在 2 秒内自愈，代价是覆盖外部手动改动（app 开关即用户意图，接受）。残留竞态：清零后 2 秒窗口内若恰逢显示状态变化触发电源评估仍可能入睡（实测 UU 场景清零到入睡有 35 秒，窗口足够）；每 2s 两次 fork（pmset -g + grep）开销约 0.5% 单核平均，接受。
- **watchdog 绑定 app 进程（pid 监视）**：崩溃安全的最小机制——app 死则系统自动恢复正常睡眠，无需落盘恢复债务、无需守护进程。代价是语义为「跟随 app 生命周期」：app 退出即失效，重启后由 config watch（immediate）自动重新持有，每次启动重新授权一次。若未来需要跨 app 生死保持，升级路径是 SMAppService LaunchDaemon + 按连接计数 hold（Keepresso 模式），当前不做。
- **授权失败回写纠偏**：取消/失败时删 flag，前端 config watch 的 catch 将 `enabled` 回写 false，防「配置说开、系统实际没开」漂移到下次启动反复弹窗。
- **启动清理**：setup 同步删 stale flag（断电/被杀场景残留；正常路径 watchdog 自愈）与旧版虚拟显示器 binary。
- **授权并发守卫**（`engaging` AtomicBool CAS）：授权弹窗模态阻塞期间二次开启会被拒绝，防 osascript 授权对话框叠加。
- **电池护栏**：`disablesleep` 会压住系统的低电量睡眠路径（Keepresso 实证：合盖 Mac 一路跑过截止线），60s 低频巡检 `pmset -g batt`，放电中低于 20% 即解除持有（删 flag，watchdog 回落），系统随即入睡；读取失败/插电一律 no-op。一次性解除无自动恢复（滞回随之不需要），重新开启由用户决定。
- **Rust 侧关闭的 config 回写**：菜单栏开关与电池护栏解除都经 `awake-enabled` 事件由 `config.ts` 模块级 listener 回写 `enabled=false`（不依赖 View 挂载），防重启后 watch immediate 误重新持有。
- **菜单栏快捷开关**：config `menubarToggleVisible`（默认 false）经 watch 同步 Rust `set_awake_menubar_visible`；开启后菜单段常驻（不随 enabled 显隐），含启用开关 CheckItem 与「熄屏方式」二级菜单（勾选态反映 enabled 与当前策略，点击切换）。开启路径走授权弹窗，取消/进行中静默（勾选态不变，重试即可）。

## 远程会话

默认零亮度策略（`dim`）即面向远程设计：背光归零、framebuffer 活跃，第三方远控（UU 远程、Screen Sharing 等）捕获流不断。清晰度受 headless 合成 framebuffer 限制（1920×1080 非 Retina，文字略虚），需要高清晰度时用系统自带虚拟显示器（系统设置 → 显示器 → 高级），不自建——虚拟屏会被 CoreGraphics 计入外接屏，若与熄屏策略并存会互相干扰。注意部分远控工具（实测 UU 远程）在会话建立时会重置非默认电源项清掉 `disablesleep`，watchdog 的持续维持会在 2 秒内写回。
