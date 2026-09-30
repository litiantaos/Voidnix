# awake

合盖不休眠扩展。核心是系统全局睡眠开关 `pmset -a disablesleep`——这是全局唯一能实现「合盖 + 无外接显示器 + 电池供电」不休眠的杠杆（`IOPMAssertion` 压不住 clamshell 睡眠路径，虚拟显示器方案又被 macOS 的 clamshell 电源前置条件挡死，已移除）。开关语义为全局睡眠禁用：开盖闲置时显示照常息屏、系统不睡；合盖时持续运行。

## 机制

三层结构：

- **常驻 LaunchDaemon**（`platform/sleep.rs::daemon_loop_script` 循环体 + `extensions/awake/native/daemon.rs` 装配）：root sh 循环内联于 plist（`/Library/LaunchDaemons/<bundle-id>.awake.plist`，dev/prod 按 bundle id 区分，不落用户可写脚本文件防 privesc），launchd RunAtLoad + KeepAlive 托管、永不退出。首次开启经 osascript 提权安装一次（`ensure_daemon`：未装 / 内容比对不一致（版本升级）/ 存活心跳过期时触发，安装后轮询心跳验证），此后跨 app 重启与重启机零弹窗。循环每 2 秒三分支：flag（`awake.flag` 稳定名）在场且 app 心跳（`awake.beat` mtime 距今 ≤ 10s）新鲜 → 持有 `disablesleep 1`（边沿置位 + `pmset -g` 持续校验自愈，见下）；flag 在场但 beat 过期 → app 已死：清 flag 恢复 0（崩溃安全，最迟 12s）；flag 缺席 → SET 守卫恢复 0 后空闲。每周期 touch `awake.daemon-beat`（app 侧安装验证与活性判定的唯一信号）。
- **熄屏巡检**（awake setup 的 2s tick）：`disablesleep` 挡住合盖睡眠后系统不会自动关内屏（无外接时 macOS 走的是睡眠路径而非 clamshell 关屏路径，背光常亮），须主动熄屏。合盖检测读 IORegistry `IOPMrootDomain` 的 `AppleClamshellState`（偶发读取失败以闭锁防抖，None 不翻转既有判定）；外接判定走 CG 活动显示列表的非内置屏；两者均为微秒级 syscall 无子进程。两档策略（config `screenPolicy`，默认 `dim`）：
  - `sleep`（显示睡眠）：合盖 + 无外接的边沿立即 `pmset displaysleepnow`（无需 root、对已灭屏幂等），停留期每 5s 节流补熄。省电最优，但显示子系统真睡、屏幕捕获流冻结——第三方远程（如 UU 远程）停摆。
  - `dim`（零亮度）：内置面板背光归零、framebuffer 保持活跃，远程控制可用。亮度经私有 `DisplayServices.framework` 的 `DisplayServicesGet/SetBrightness(display, float)` 读写（Keepresso 生产验证的同款通道；dlsym 解析，display id 取 online→active→进程内缓存——合盖后两个 CG 列表都会摘掉内置屏）。恢复目标在开盖期持续采样（macOS 合盖首拍即归零，活读常为 0），开盖（或开关关闭）时面板仍暗才恢复（外部已调亮的不覆盖）；符号缺失/无内置屏自动退化显示睡眠。零亮度残留时关闭开关会先还亮度再退场（防开盖黑屏）。**本机新背光架构实测**：IORegistry 直写 `AppleARMBacklight`（32/64 位 CFNumber 均同）与 `CoreDisplay_Display_SetUserBrightness` 都只改报告值不驱动实际背光，且读回验证全部假阳性（写入后读回一致仅说明服务层状态变了）——亮度通道的有效性以物理观察为准，诊断日志带写入返回值。
- `extensions/awake/native/mod.rs`：意图状态（`AtomicBool`）+ app 心跳任务（enabled 时按 daemon 周期 touch beat；自愈——enabled 而 flag 缺失且 daemon 活着时补写 flag，修复强制睡眠唤醒后 daemon 已按 beat 过期自清、app 真值仍 true 的漂移；daemon 已死则不补，留待下次 engage 重装）。engage 顺序硬不变量：`ensure_daemon`（可能提权弹窗，可达分钟级）→ touch beat → 写 flag → 置 enabled——心跳任务仅在 enabled=true 时 touch，flag 先于弹窗落盘会被在场 daemon 按 beat 过期自清（静默漂移）；取消/失败发生在写 flag 之前，零清理。

## 关键决策

- **flag 文件而非每次直写**：安装一次后开关即纯文件操作。daemon 为**持续维持**而非边沿写——持有期间每周期校验 `pmset -g` 的 `SleepDisabled`（输出为 `SleepDisabled\t\t1`，匹配须桥接 tab），失配即重写。纯边沿写有实测事故：全局设置被外部清零后持有静默失效，系统按 Clamshell Sleep 入睡（远程会话表现为锁屏解锁瞬间离线）。清零源已实验实锤为 UU 远程——每次会话建立时重置非默认电源项（对照实验：置 1 后两次 UU 连接即清零，静置无连接 4 分钟保持 1）；另一实例退出恢复、手动 `sudo pmset` 同为可能源。持续维持使外部清零在 2 秒内自愈，代价是覆盖外部手动改动（app 开关即用户意图，接受）。残留竞态：清零后 2 秒窗口内若恰逢显示状态变化触发电源评估仍可能入睡（实测 UU 场景清零到入睡有 35 秒，窗口足够）；每 2s 两次 fork（pmset -g + grep）开销约 0.5% 单核平均，接受。
- **常驻 LaunchDaemon 而非 pid 绑定 watchdog**（升级动机：原设计 app 死即失效，每次重启 app 首次开启重新授权弹窗，config watch immediate 自动恢复持有使弹窗出现在启动时刻）：崩溃安全改由 app 心跳承载——beat 停更 GRACE（10s，5 周期余量容忍 worker 短暂阻塞）后 daemon 自清 flag 恢复默认睡眠，无落盘恢复债务。app 退出语义同前（最迟 12s 恢复），重启间隙短于 GRACE 则无缝续持零弹窗。代价：曾开启过的机器常驻一个空闲 root sh 循环（空闲分支零 fork，开销可忽略）；手动卸载 `sudo launchctl bootout system/<label> && sudo rm /Library/LaunchDaemons/<label>.plist`，下次开启自动重装。时钟前跳 → 单周期伪释放、2s 自愈；时钟后跳 → age 为负判新鲜（安全方向）。多用户边界：LaunchDaemon 系统级而数据目录按用户，后装用户覆盖前装 daemon（单用户自用假设）。plist 内容比对版本化：草稿与安装件（644 全局可读）字节相等且心跳新鲜才跳过安装，循环逻辑升级 / 数据目录迁移自动重装。
- **授权失败回写纠偏**：取消/失败发生在写 flag 之前（engage 先安装后落 flag），前端 config watch 的 catch 将 `enabled` 回写 false，防「配置说开、系统实际没开」漂移到下次启动反复弹窗。
- **启动清理**：setup 保留旧版 pid 命名 flag 的 glob 清理（升级前残留）与旧版虚拟显示器 binary；新版残留 flag（断电双杀 app 与 daemon 的极端场景）由 daemon 重生后首周期按 beat 过期自清。
- **授权并发守卫**（`engaging` AtomicBool CAS）：授权弹窗模态阻塞期间二次开启会被拒绝，防 osascript 授权对话框叠加。
- **电池护栏**：`disablesleep` 会压住系统的低电量睡眠路径（Keepresso 实证：合盖 Mac 一路跑过截止线），60s 低频巡检 `pmset -g batt`，放电中低于 20% 即解除持有（先撤意图再删 flag——心跳补写任务按 enabled 决定是否补 flag，顺序颠倒会被复活；daemon 回落），系统随即入睡；读取失败/插电一律 no-op。一次性解除无自动恢复（滞回随之不需要），重新开启由用户决定。
- **Rust 侧关闭的 config 回写**：菜单栏开关与电池护栏解除都经 `awake-enabled` 事件由 `config.ts` 模块级 listener 回写 `enabled=false`（不依赖 View 挂载），防重启后 watch immediate 误重新持有。
- **菜单栏快捷开关**：config `menubarToggleVisible`（默认 false）经 watch 同步 Rust `set_awake_menubar_visible`；开启后菜单段常驻（不随 enabled 显隐），含启用开关 CheckItem 与「熄屏方式」二级菜单（勾选态反映 enabled 与当前策略，点击切换）。开启路径仅 daemon 首次安装走授权弹窗（此后零弹窗），取消/进行中静默（勾选态不变，重试即可）。

## 远程会话

默认零亮度策略（`dim`）即面向远程设计：背光归零、framebuffer 活跃，第三方远控（UU 远程、Screen Sharing 等）捕获流不断。清晰度受 headless 合成 framebuffer 限制（1920×1080 非 Retina，文字略虚），需要高清晰度时用系统自带虚拟显示器（系统设置 → 显示器 → 高级），不自建——虚拟屏会被 CoreGraphics 计入外接屏，若与熄屏策略并存会互相干扰。注意部分远控工具（实测 UU 远程）在会话建立时会重置非默认电源项清掉 `disablesleep`，daemon 的持续维持会在 2 秒内写回。
