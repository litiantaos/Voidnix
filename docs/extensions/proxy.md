# proxy

Clash 风格代理扩展（统一 TUN 模式）。运行时按需下载 mihomo（Clash.Meta）核心，扩展负责 **root 进程托管 + 订阅解析 + mihomo controller 转发 + 热重载 active/idle 切换**。TUN 由虚拟网卡接管全部 IP 流量（含命令行/Docker/原生应用），是完整的全局代理。

## 架构

```
前端（Vue，单主界面分组列表）        Rust（native/）
View.vue（模板）+ useProxyPanel.ts（状态/动作）+ logic.ts / config.ts
Actions.vue（搜索栏诊断入口）+ views/ 三诊断子视图（连接/规则/日志）
  ├ 代理组：开启/模式    ──invoke──→ mod.rs 命令入口
  ├ 订阅组：导入/更新/删除           ├ lifecycle.rs  状态机（启停/热重载/健康监测/启动复用）
  └ 节点组：列表/切换/测速/分组切    ├ menu.rs       菜单栏贡献
                                     ├ core.rs       binary 写盘 + 版本
                                     ├ subscription.rs 拉取+合并 Clash YAML
                                     ├ controller.rs ──reqwest──→ mihomo API
                                     ├ tun.rs        osascript 提权 root
                                     └ stream.rs     traffic/connections/logs WS
```

**端口与协议**：

- mihomo 监听 **`mixed-port`**（HTTP+SOCKS5 共用）
- mihomo 监听 **`external-controller`**（RESTful API，bearer secret 鉴权）
- 扩展**不解析代理协议**，proxies/proxy-groups/rules 原样合并自订阅 Clash YAML

**UI 结构**：无独立设置子视图，全部控制内联在主界面的三个分组中：

- **代理分组**：含开启/规则模式两项；开启项副标题经 `/traffic` WS 实时显示上下行速率
- **订阅分组**：导入/更新/删除；多订阅时仅激活订阅生效（点击有节点的订阅行切换激活，accent 强调当前激活项；空订阅点击进编辑，编辑按钮随时进编辑）。行内「更新」按钮按现有 URL 重新拉取（与弹窗保存共用 `proxy_update_subscription`；激活订阅更新后节点列表整体替换并清测速缓存，非激活订阅仅回填元数据），新建订阅拉取成功自动激活
- **节点分组**：列表/切换/测速；多 selector 分组订阅时节点组首项出现分组切换器

**搜索过滤**：主搜索栏统一过滤当前视图——主界面按名过滤节点，诊断子视图过滤连接/规则/日志（切视图清空查询）。

**诊断入口**（搜索栏右侧，toggle 切换）：连接/规则/日志三个子视图：

- 分别接 mihomo **`/connections`**（实时连接列表）、**`/rules`**（分流规则只读列表）、**`/logs`**（实时日志，环形缓冲 500 行，全级别推送由搜索过滤）
- 三个子视图均用 BaseList

**菜单栏**：图标仅在已连接时显示（打开扩展 + 已连接状态可点断开）；其余控制全部在扩展视图。

## mihomo binary 下载（运行时按需）

**不编译期嵌入**（避免 Voidnix 二进制膨胀 + 可热更新），改为运行时按需下载。代价：首次使用需联网。

### 下载流程

**`core::ensure_bin`**：binary 已存在即复用（不校验版本，避免无谓重下），否则下载。

**并发保护**：并发触发时（双击/开代理 spawn），用 **tokio Mutex + double-check** 串行化——仅一个真正下载、其余抢锁后复用，防多流写同一 `mihomo.gz` 损坏致 sha256 失败、binary 无法产出。

**`download_core_async`** 仅组装 BinaryFetch spec，下载走共享管线 **`runtime/binary_fetch`**（与 video 共用：多 URL 回退 + sha256 校验 + gunzip + chmod；内部经专用 `http::download_client` reqwest 流式拉取）：

- **无整体超时**，仅建连 30s——慢网络下 15MB gz 下载耗时不可控，全局 `HTTP_CLIENT` 的 120s 整体超时含 body 读取会中途掐断流
- 国内镜像 **`gh-proxy.com`** 前缀（镜像仅代理转发，sha256 保证内容一致）

**进度事件**：逐 chunk 推送 **`proxy-core-progress`** 事件，payload `{ received, total }`。前端按钮文案三态：

- **连接中**：未收首字节
- **N%**：有 Content-Length 且未收齐
- **解压中**：字节收齐后 Rust 再 emit 一次 `total=Some(received)` 完成信号（覆盖 sha256+gunzip 后处理阶段）
- 镜像回源 chunked（`total=None`，实测 gh-proxy 透传 content-length 不触发）时诚实回退显示已收字节

### 校验与解压

下载完成后：**sha256 校验**（sha2）→ **`gunzip` 解压** → **chmod 0o755**。

### 版本源与升级

**`fetch_latest_asset`** 直连 `api.github.com/repos/MetaCubeX/mihomo/releases/latest`，单次调用拿：

- **`tag_name`**（版本）
- **`assets[*].digest`**（内嵌 sha256，无需独立 digest 文件）

asset 名精确串等 **`mihomo-darwin-{arch}-{tag}.gz`**，排除 go120/go122/go124 变体。

**API 不可达时**：`fallback_asset` 用常量（`MIHOMO_VERSION` + `SHA256_ARM64`/`SHA256_AMD64`）拼装，首次下载仍可用（仅无法获取新版）——gh-proxy 仅代理 release 下载、不转发 API。

**升级走 UI**：

1. 进视图时 **`proxy_check_update`** 后台拉 latest 比对本地 `mihomo.version`
2. 有新版时副标题显示「核心 vX.Y.Z（最新 vZ，可更新）」+ 出现「更新」按钮
3. 点击触发 **`proxy_update_core`**：`tun_active` 时先 `uninstall_launchdaemon`（bootout + 删 plist，停进程）→ 删 binary + version → `ensure_bin` 重下最新 → 之前 enabled 则 `start_core` 重新安装恢复
4. 中途下载失败：文件已删，下次 `ensure_bin` 自动重试

### 下载状态

下载状态真相源在 Rust：

- **`DOWNLOADING`** 原子标记下载中，`core_status` 据此返回
- 前端 `isDownloading` = `coreStatus.downloading`（computed）
- 重新进入视图不调 `loadCoreStatus`（调用点仅 downloadCore/updateCore 成败与 `proxy-core-ready` 事件）：KeepAlive 命中靠 ref 保留乐观值，LRU 驱逐重挂载读模块级启动快照（preloaded），据此反映下载中/已下载/未下载

**进度数值不持久化**（低频一次性操作）——退出重进时若仍在下载，按钮显示「下载中」，收到下一个 `proxy-core-progress` 事件即恢复具体百分比。

**就绪信号**：gunzip/chmod/version 全部就绪后 Rust emit **`proxy-core-ready`**，前端事件驱动 `loadCoreStatus` 刷新——不依赖 `invoke(proxyEnsureCore)` 的 resolve 时序（sha256/gunzip 同步阻塞可能延迟 IPC 响应，曾导致前端乐观标记的 `downloading=true` 不被刷新、UI 卡在「解压中」）。

**下载与启用解耦**：`downloadCore` 不自动 `toggleEnabled`，核心就绪后用户手动开启。

### Geo 数据库

**`ensure_geo_files`**：mihomo 加载含 GEOIP/GEOSITE 规则的 config 时需 **`geoip.metadb`** + **`geosite.dat`**，缺失触发同步下载（直连 GitHub 国内不可达 → EOF → config 加载失败/控制器不启动 → 开代理超时）。

`install_launchdaemon` 前置 `ensure_geo_files` 经 gh-proxy 镜像预下载——已存在跳过，失败不阻塞（mihomo 经 `geox-url` 自行重试）。

## 运行模式（统一 TUN）

mihomo 以 root 经 **launchd LaunchDaemon 托管**（`/Library/LaunchDaemons/<bundle-id>.mihomo.plist`）常驻——TUN 需 root 创建虚拟网卡 + auto-route，接管全部 IP 流量。首次开启代理时 `tun::install_launchdaemon` 经 `osascript ... with administrator privileges` 提权**一次**安装 plist 并 bootstrap 启动；之后 RunAtLoad 开机自启 + KeepAlive 崩溃自愈，Voidnix 全程经 controller API 热重载 active/idle config 控制，**日常零提权**。

### LaunchDaemon plist

plist 的 `ProgramArguments` 指向 mihomo binary（绝对路径）+ `-d` 数据目录；mihomo 以 root 跑、读数据目录的 `config.yaml`。`KeepAlive=true`（进程退出即重启）+ `ThrottleInterval=30`（限制崩溃重启频率，降低极端情况下拉起刷日志）。plist 须 `chown root:wheel` + `chmod 644`，否则 launchd 拒绝加载。label 按 bundle identifier 区分 dev/prod。

### TUN 独占与进程清理（install 时一次性）

**TUN 是系统独占资源**（虚拟网卡 + 路由 `1.0.0.0/8` 等），两个 mihomo 实例不能同时占 TUN（`add route: file exists`）。`install_launchdaemon` 安装时**只清理自己的 mihomo**（按 binary 完整路径匹配，含 bundle-id 数据目录），不杀别的实例——dev/prod 端口隔离互不占用，可同时常驻。与第三方工具的冲突由三层诊断处理（见下）。

**dev/prod 端口隔离**：dev 构建默认 `7891/9091`，prod 默认 `7890/9090`（`import.meta.env.DEV` 偏移 +1）。两个 mihomo 可同时常驻（idle/idle 或 idle/active）互不干扰。

**dev/prod TUN 让渡**：TUN 独占意味着同一时刻仅一个变体能 active。对端变体残留 active 占 TUN 时（典型：dev 开着代理退出 app——mihomo 由 launchd 继续托管保持 active，对端 UI 已不在无从关闭），本端开代理不按第三方报错，而是**优雅让渡**：由 bundle identifier 推导对端数据目录（差 `.dev` 后缀），读对端 `config.json` 的 controller 端口 + secret（端口按对端视角归一化），`GET /configs` 确认对端 `tun.enable=true` 后 PUT 其磁盘上的恒 idle `config.yaml` 热重载——TUN 随之拆除，全程免提权、只读访问对端目录。让渡后轮询 auto-route 路由撤除（最多 3s）再激活本端。对端 idle 常驻（`tun.enable=false`）说明占用者是第三方工具、对端进程未跑、或对端不可控（凭证缺失/secret 不匹配）时，维持原报错路径（提示打开对端版本关闭代理）。对端 app 若在跑，让渡成功即发布 **macOS 分布式通知**（`platform/distributed.rs`，NSDistributedNotificationCenter 原生跨进程总线，发布者 object = 变体 bundle id、观察者按 object 过滤自收排除）——对端毫秒级收到推送、验证运行 config 后即时重置 UI 并提示重开（半路由无法判归属——接管后路由仍在只是换了主人）；未收到通知的脱节路径由健康监测 30s 不变式对账兜底（见下）。

**端口归一化（三层防御）**：config.json 可能残留对端变体默认端口（历史污染 / 手动复制 / `defineConfig` 异步 backfill 覆盖），需在多个层面修正：

- **Rust 权威层**（`core::correct_variant_ports`）：`set_proxy_enabled` 命令入口 + `read_run_params`（reconnect 直读 config.json）处用 `cfg!(debug_assertions)` 静默修正——`cfg!` 与 Tauri bundle identifier 一致（debug 构建自动追加 `.dev`），是变体判定的权威源
- **前端结构层**（`config.ts` 模块级 watch）：app 启动即注册（非 composable 内延后），`flush: 'sync'` 确保 `defineConfig` backfill 回填错误端口时在同一同步栈内即时修正，消除 backfill 到修正之间的窗口
- **磁盘层**：前端 normalizer 修正后经 defineConfig 300ms 防抖持久化，后续 config.json 自动修正

**idle 不占 TUN**：Voidnix mihomo 常驻 idle 时 TUN 空闲，不影响切到其他代理软件，仅开代理（active）时独占；但 idle 仍占 mixed-port/controller 端口，端口相同会与其他代理工具冲突（见下）。

### 冲突处理（三层）

与 Clash Verge / Mihomo Party / ClashX 等其他代理工具共存时，端口（7890/9090 行业默认）与 TUN（系统独占）会冲突。三层处理把「静默强杀别人 + 笼统失败」变为「探测识别 + 明确反馈 + 用户决策」：

**第一层 · 端口预探测**（install 前，免提权）：`port_occupant` 经 `lsof -iTCP:<port> -sTCP:LISTEN` 查 mixed-port/controller 占用者。被**别的程序**占 → 直接报错（`7890 端口被 mihomo（PID 1234）占用，请先关闭它或修改端口`），不进 osascript 不强杀。

**第二层 · 启动失败诊断**（wait_ready 超时后，免提权）：`diagnose_launch_failure` 读 `mihomo.log` 尾部识别已知错误模式（`address already in use` / TUN `file exists`）+ 再跑一次 `lsof` 查端口占用者，拼成可操作提示（`TUN 网卡或路由被其他代理工具占用`），而非笼统的「启动失败」。

**第三层 · fatal 回收 + 循环抑制**（install 脚本内，同一提权 session）：bootstrap 后 `curl` 轮询验 controller /version（secret 匹配，0.2s 间隔 ×10，首次成功即 break）——mihomo 绑定端口失败时**不退出**（降级运行无监听），`pgrep` 误判成功；controller API 健康检查才能识别降级实例。检测到不可用则在**同一提权 session** 内 `bootout` + 删 plist——从根源消除 KeepAlive 反复拉起刷日志，一次提权完成「装 + 验证 + 失败回收」。`ThrottleInterval=30` 与 KeepAlive 配合进一步降低极端情况下拉起频率。条件 kill（有匹配 pid 才 sleep 1）+ curl 轮询替代固定 sleep，首次安装从 ~4s 降至 ~1s。install 返回前再从 Voidnix 进程 `wait_ready` 复验 controller（curl 在 osascript root shell，与 Voidnix 的 reqwest 不同执行上下文；mihomo 刚 bootstrap 后 providers/geo 初始化有短暂抖动窗口，root shell curl 命中不代表本进程首次连接必达，wait_ready 用同一 CONTROLLER client 确认连接就绪为紧随的 reload 铺路）。

**热重载路径 TUN 冲突防护**（三层）：(1) **路由预检**（`tun_route_conflict`，start_core 前置）：`netstat -rn` 检测 TUN auto-route 路由——老版半路由 `0/1` + `128/1` 与新版路由树分解 `1` + `2/7` + … + `128.0/1`（避开 0.0.0.0/8）两代风格均识别，存在则先尝试 dev/prod 让渡（见上），让渡不成（第三方占用）直接拒绝。(2) **同步 TUN 验证**（`verify_tun_active`，热重载后 200ms）：mihomo PUT /configs 返回 204 不代表 TUN 创建成功（别的工具占路由时静默失败），读 mihomo.log 新增行检测 TUN/route error。start_core 和 proxy_reconnect 均**同步调用**（失败时即时回滚 idle config 清理 mihomo 状态，避免遗留 broken active config 致 controller 无响应、后续重开走 osascript 重装）。(3) 预检漏过 + verify 也漏过时，健康监测 30s 探针兜底。

### 进程管理

mihomo 生命周期由 launchd 托管（KeepAlive 保活），无裸进程 spawn/kill。binary 升级/卸载走 **`tun::uninstall_launchdaemon`**（提权 bootout + 删 plist）。

**config.yaml** 含 **`tun`**（system stack + dns-hijack + auto-route）+ **`dns`**（fake-ip）段。stack 选 system（非 gvisor）：走 macOS 原生 utun + 内核 TCP 栈，gvisor 用户态栈在连接风暴 + 批量超时失败时会泄漏 dial goroutine 进入 busy-loop（睡眠唤醒后数十 App 重连触发，CPU 卡 100% 不自愈），system 将连接管理交还内核从根上消除该泄漏。

### osascript 期间双抑制

**`run_osascript`** 调用期间双抑制：

- **`click_monitor::suppress`**：暂停 click-outside（SecurityAgent 授权框在主窗口外，用户点击输密码/确认会被判为 click-outside 触发 hideWindow）
- **置位 `focus::OSASCRIPT_RUNNING`**：授权完成 SecurityAgent 关闭后 shell 命令仍跑 2-3s，frontmost 已还给原 app 但 is_app_active 仍返 true 抑制 blur hide
- 收尾在主线程 make_key 恢复 panel 焦点 + 清 flag

### launchd 常驻 + 热重载 active/idle（日常零提权）

osascript 每次提权都弹系统密码框。launchd 托管把提权收敛到「**首次安装 plist 一次**」，之后进程永驻、开关走热重载：

- **首次开代理**：`ensure_root_mihomo` 检测 plist 未装 → `install_launchdaemon`（提权 1 次：端口预探测 + 只杀自己的旧 mihomo + 装 plist + bootstrap，mihomo 启动跑 idle config）→ `start_core` **TUN 路由预检**（对端变体占用则让渡接管，第三方则直接拒绝）→ 热重载 active config → **同步 TUN 验证**（失败时回滚 idle config）
- **关代理**：`stop_core` 先写 **idle config.yaml** 到磁盘（即使 controller 卡住，mihomo 重启后自动加载 idle），再短超时（3s）单次热重载。成功则 TUN 即时释放；失败 + mihomo 在跑时**乐观返回成功**（UI 即时显示关闭），后台异步重试释放 TUN（全部失败才 toast）；失败 + mihomo 已死则视为已关闭。**进程保留**（launchd 继续托管），可 `proxy_reconnect` 重试
- **再开代理 / 开机后首次**：`ensure_root_mihomo` 命中「plist 已装」→ controller 可达则直接复用，不可达则等 launchd 拉起（KeepAlive）→ 热重载 active config，**免提权**

**效果**：密码从「每次开关 / 开机后首次都要弹」→「首次安装 1 次，之后永久 0 次」（仅 binary 升级/卸载再提权）。

**代价**：mihomo 进程永久常驻（idle ~50MB，不代理流量，用户无感；idle 不占 TUN，与其他代理软件共存）。

### app 启动复用

app 启动时 **`reconnect_root_mihomo`** 检测 launchd 托管的 mihomo（按 binary 路径 ps 查）：

- 先 **`GET /version`** 验 secret——匹配后 **`GET /configs`** 按运行 config 的 `tun.enable` 分流：
  - **active**（app 退出前开着、launchd 保活至今）→ **恢复 `enabled` + `run_params` + 重启健康监测 + emit `proxy-enabled:true`** 同步前端/菜单，并异步拉取当前节点名填充菜单状态行（与开启路径对齐，best-effort）——常驻设计的意图是 app 退出不影响代理，重启 app 静默切直连等于无提示的流量裸奔（mode 以运行 config 为权威）。前端挂载时再查一次 `is_proxy_enabled` 兜底（reconnect 在 Rust setup 异步跑，可能晚于模块预加载，emit 时面板未挂载会丢失）
  - **idle** → 热重载 idle 幂等复位直通，成功才标记 `tun_active` + 设 `run_params`（子视图诊断只需 controller 可达，不依赖代理开启）
- secret 不匹配（401 = 不可控，如旧 osascript 残留）**不提权清理**（避免 app 启动弹窗），下次开代理时 `install_launchdaemon` 会清理本端路径的 mihomo 实例并接管

### `ensure_root_mihomo` 三分支信任

**`ensure_root_mihomo`** 确保可信的 mihomo 在跑，按优先级：

1. **controller 可达 + secret 匹配** → 复用（reconnect 成功 / 本 session 已开），置 `tun_active`，免提权
2. **plist 已装但 controller 不可达**（开机后/崩溃重启中）→ `wait_ready` 等 launchd 拉起（KeepAlive），免提权；等不到（plist 损坏/加载失败）回退 3
3. **plist 未装** → `install_launchdaemon` 提权一次（清理本端路径的 mihomo 实例 + 装 plist + bootstrap）

mihomo 输出重定向到 **mihomo.log**（启动失败可查日志）。

> 历史上有过 user 模式（系统代理）+ TUN 模式双选；因 user 模式仅覆盖遵守系统代理的应用（命令行/Docker/部分原生应用不走代理）非完整代理，已移除，统一为 TUN。早期 TUN 经 osascript 每次提权 spawn/kill 裸进程，密码框频繁；现改为 launchd 托管，提权收敛到首次安装一次。

## 健康监测 + 自动热重载恢复

root mihomo 常驻但进程可能因出站失效/接口抖动/异常退出而「假死」——内存状态（enabled/tun_active）与实际脱节：UI 仍显示已开启，用户靠测速才发现全超时；历史上关闭重开还因热重载失败需两次提权重启进程，launchd 托管后虽免提权，仍需手动操作且中断代理。

### 监测机制

`start_core` 成功后 spawn 健康监测 task（**`ensure_monitor`**，幂等），**单频 30s 循环两职**：

- **不变式对账**：读运行 config 的 `tun.enable`，`enabled=true` 但 tun.enable=false = 状态脱节（核心崩溃后 KeepAlive 重启进 idle / 外部持 secret 改动配置 / 让渡未及通知），流量实际直通——复位 + 通用提示。controller/测速探针对此无感（idle config 仍含全部节点），半路由也无感（接管后路由仍在只是换了主人），读运行 config 才能识别
- **出站探针**（`probe_health` = controller GET /version 可达 + 当前主 selector 节点 delay test 出站可达），**连续 2 轮异常**（容忍单次抖动）才动作
- **代际退出**（`monitor_gen` 计数）：stop/reset 自增代际使在跑 task 失配即退、`ensure_monitor` 按「当前代际已有 task」幂等跳过——共享 bool 标志下旧 task 醒来会读到新 task 置位的 true 而「复活」（stop 后 30s 内重开 → 双 monitor 并存，重复探针/恢复/toast）

**脱节检测的事件化分层**：能拿到事件源的直接推送——让渡经 macOS 分布式通知即时对账（精确文案「被另一版本接管」）；stop 后台释放重试与重开的竞态在源头消灭（`release_gen` 代际，start_core 入口自增使挂起重试自弃）；其余无事件源的路径（核心重启/外部改动）由上述 30s 对账兜底（通用文案「TUN 未生效」）。无常驻高频轮询。

### 恢复动作

- **进程已退出**（`!root_mihomo_running`）或 controller 不可达 → **`reset_dead_state`**：复位 enabled/tun_active 两标志 + `invalidate_monitor`（监测代际失效）+ `StreamRegistry::cancel_all` 停流 + emit `proxy-enabled:false` + emit `proxy-status{kind:error}`（前端状态栏提醒 + 开启项红色提示）+ menubar refresh 隐藏图标。**不自动提权重启**（避免突兀弹密码框），由用户手动重新开启
- **进程在 + controller 在 但出站死** → **免提权热重载 active config**（`reload_config_yaml`，PUT /configs 让 mihomo 重建 TUN 栈/连接池/接口绑定，对症「重启就好」）；热重载失败 emit `proxy-status{kind:error}` 通知，下轮重试
- **状态脱节 / 被对端变体接管**（enabled 但运行 config `tun.enable=false`）→ `reset_dead_state`——本端 mihomo 已被热重载 idle 或核心重启回退 idle，流量实际直通。让渡路径经分布式通知即时发现（文案「TUN 已被另一版本 Voidnix 接管」），其余路径由 30s 对账发现（文案「代理已断开（TUN 未生效）」）

### 手动重连

用户亦可手动触发 **`proxy_reconnect`**（免提权软重启，UI 开启项「重连」按钮）：进程在 + controller 可达时一键热重载 active config——launchd 托管下关闭重开虽免提权，但热重启更快且不中断代理。进程已退出则报错提示需关闭重开（会提权）。

`stop_core` 经 `invalidate_monitor` 停监测（用户主动关闭无需监测）。

## 订阅合并（subscription.rs）

### 单激活模型

同一时刻仅一个订阅生效——前端 `config.activeSubscriptionId` 标记激活订阅，`build_run_config` 仅读取 `subs/<active_sub_id>.yaml` 参与合并，未激活订阅的 YAML 仅缓存待激活。节点列表只呈现激活订阅的节点，避免多订阅节点/分组混杂。

激活订阅 id 经显式命令（`set_proxy_enabled` / `proxy_set_active_subscription` / `proxy_remove_subscription`）传入 Rust 的 `RunParams.active_sub_id`，**不读 config.json**——规避前端持久化 300ms 防抖窗口内 Rust 读到旧值的竞态。前端 normalizer watch 保证 `activeSubscriptionId` 始终指向有效 id（失效回退首项）。

### 合并规则

**`merge_yaml(texts, params)`**（纯函数）：

- 多文本 **proxies** 按 name 去重拼接（单激活模型下通常仅 1 份，去重作单订阅内重名防御）
- **proxy-groups / rules** 取首个非空文本，否则自动生成（groups 为 `节点选择` select + `自动选择` url-test；rules 为 `GEOIP,CN,DIRECT` + `MATCH,节点选择` 两条）

订阅自带顶层字段被丢弃（仅取 proxies/groups/rules）。

### 全局性能开关（恒注入）

框架恒注入以下全局性能开关（订阅自带顶层字段被丢弃，故这些开关必须由框架注入而非依赖订阅）：

- **`unified-delay: true`**：测速扣除 DNS+TCP+TLS+协议握手耗时，只留 HTTP RTT，对齐 Clash Verge Rev / Mihomo Party 等默认——缺失此项致 ANYTLS 等 TLS 协议握手 300-600ms 被计入延迟，数值偏高一个数量级
- **`tcp-concurrent`**：多 IP 节点并发建连取最快
- **`keep-alive-interval: 30`**：连接保活，降重复握手与测速波动

### 测速 URL 强制覆盖

订阅自带的测速型分组（`url-test` / `fallback` / `load-balance`）的 **`url`** 一律覆盖为 `https://cp.cloudflare.com/generate_204`：

- mihomo 自警告 HTTP 测速 URL 会被机场/ISP 劫持（"hijacking test addresses, using HTTP may result in failed tests"），实测致测速失败/数值偏高 3-4 倍
- HTTPS 不被劫持
- cp.cloudflare.com 国内 DNS 解析得真实 Cloudflare IP 不污染，海外 anycast 延迟最低

interval/tolerance/lazy 等其他字段保留订阅原值。

### DNS 配置

- **nameserver**：国内直连（223.5.5.5/119.29.29.29，fake-ip 查询 + DIRECT 流量真实解析均走此，国内 DNS 对常见海外域名如 apple.com 返回正确 IP）
- **`proxy-server-nameserver`**：节点域名专用，保证 TUN 排除路由可靠添加防回环
- **不配 fallback/fallback-filter**：海外 DoH 在 TUN 下经代理，DIRECT 海外域名解析会串行等待 fallback（实测 apple.com couldn't find ip），是 active 比 idle 测速慢一个数量级的根因；被污染域名走代理远程解析，不依赖本地 DNS

config 含 **`geox-url`**（geoip/geosite 镜像 URL，国内直连 GitHub 不可达致 mihomo 下载 EOF）。

订阅原文存 **`subs/<id>.yaml`**，`build_run_config` 启动时读取合并。

### 订阅拉取与热重载

- 订阅拉取走 **`http::client()`**（SSRF 校验 + Clash UA `clash.meta/v1.19.27`，确保机场返回 YAML 而非 Base64）
- 更新入口两个（编辑弹窗保存 / 订阅行内「更新」按钮），共用 `proxy_update_subscription` 命令；行内更新单飞（任一更新中其余行按钮禁用，进行中行转 spinner）
- 切换激活订阅 / 增删订阅触发 **`reload_running_config`**（按 enabled/idle 自适应重载）：重建 `config.yaml` 后 `PUT /configs {path}` 让 mihomo 原生热重载（root 进程常驻，免重启免再提权）。激活订阅切换即整体替换节点列表（清空乐观选中与测速缓存）

## mihomo controller 转发（controller.rs）

独立 reqwest 客户端（不经 `http::client()` 的 SSRF 防护——controller 固定 127.0.0.1 本地回环）。路径段经 **`urlencoding::encode`**（支持 emoji 分组名）。

**REST 端点**：

- `GET /proxies`（节点列表）
- `PUT /proxies/{group}`（选节点）
- `GET /proxies/{name}/delay`（单节点测速；流式批量测速 `proxy_test_group_delay_stream` 与健康探针 `probe_health` 均复用此端点，替代批量端点 `/group/{group}/delay`——后者需等全组结算一次返回，被死节点拖累）
- `PATCH /configs`（切模式）
- `PUT /configs {path}`（热重载配置，**重试 3 次间隔 300ms**——install 路径 mihomo 刚 bootstrap 首次连接撞初始化抖动窗口时自愈；HTTP 错误码不重试）
- `GET /rules`（分流规则只读快照）

## 连接 / 流量 / 规则 / 日志（诊断，stream.rs）

mihomo controller 的 WS 流式端点（`/traffic` `/connections` `/logs`）经 Rust 桥接 + **`tauri::ipc::Channel<T>`** 推前端（与 agent Channel 同范式）；`/rules` 走 controller REST。

### 流生命周期

**`StreamRegistry`**（仿 agent `SessionRegistry`，`CancellationToken` 注册中心）管理流生命周期：

- 前端按需开（进视图传 Channel）、离开调 `proxy_stop_stream`
- `stop_core` / `reset_dead_state` 调 **`cancel_all`** 兜底（关代理/进程退出时停所有 WS，免 idle 残留空转）
- WS 鉴权用 **`?token={secret}`** query（mihomo 支持，bearer 不适用于 WS Upgrade）
- 三条流均本地回环，连接失败静默退出（前端可见时才开，无感重开）

前端子视图经 KeepAlive 缓存（activate/deactivate 而非重挂载），流生命周期绑 `onActivated`/`onDeactivated`：切走即停 WS、切回即重开（避免不可见时空转）。

### 四条流

- **流量**（`/traffic`，每秒 `{ up, down }` bytes/s）：主界面开启项副标题实时显示，代理开关驱动（前端开/停）
- **连接**（`/connections?interval=500`，完整快照含 connections 数组 + 累计总量）：子视图实时列表，搜索 host/进程/IP
- **规则**（`/rules`，`{ rules: [{ type, payload, proxy }] }`）：子视图只读列表，支持规则/策略搜索
- **日志**（`/logs`，`{ type, payload }`，WS `level=debug` 拉取 mihomo 产出的全部级别；config `log-level=info` 故实际为 info+）：子视图实时流，前端上限 500 行超出头部丢弃，按 type 着色（error red / warning yellow / debug muted），搜索过滤 payload，贴底自动滚（用户上滚查历史时不打断）

## 聚合菜单栏贡献（mod.rs）

代理已连接时向框架统一菜单栏托盘（`runtime/menubar.rs`，`public/bar_icon.png` 模板图）贡献两项——极简 + 唯一（控制逻辑全部在扩展视图，菜单不重复）；断开后 `build` 返回空，图标自动隐藏。`setup` 内 **`menubar::register`** 声明 `build`/`on_event`，状态变更后 **`menubar::refresh`** 重建。

**两项贡献**：

- **打开扩展**（Item，点击打开代理视图）
- **已连接：节点**（CheckItem 勾选，点击断开 → 图标隐藏，重连走扩展视图）

状态行当前节点名由 **`refresh_proxy_menu`** 异步拉 `controller::get_proxies` → `parse_current_node`（取主 selector 的 `now`）填充缓存（`ProxyState.current_node`）；`set_proxy_enabled` / `proxy_select_proxy` / `proxy_update_subscription` / `proxy_remove_subscription` / `proxy_set_active_subscription` 五个命令入口在调用后 spawn 刷新（`reload_running_config` 本身不触发）。点击状态行调 `stop_core` 热重载 idle 断开代理，emit `proxy-enabled:false` 同步视图 + refresh 使 `build` 返回空 → 图标隐藏。其余控制（模式/订阅/节点切换/测速）仍在扩展视图。

## 命令（19 个）

- **启停**：`set_proxy_enabled`（launchd 托管 mihomo——首次 install_launchdaemon 提权一次，之后开关走热重载 active/idle config 免提权；传 `active_sub_id` 指定激活订阅）/ `is_proxy_enabled`
- **核心下载**：`proxy_core_status` / `proxy_ensure_core`（核心版本查询与运行时按需下载）
- **版本升级**：`proxy_check_update` / `proxy_update_core`（拉 GitHub API latest 比对版本 / 停代理 + 删旧 + 重下 + 恢复）
- **订阅**：`proxy_update_subscription`（订阅 + 热重载）/ `proxy_remove_subscription`（删订阅 + 切新激活 + 热重载，传 `new_active_sub_id`）/ `proxy_set_active_subscription`（切激活订阅 + 热重载）
- **节点与测速**：`proxy_get_proxies` / `proxy_select_proxy` / `proxy_test_group_delay_stream`（流式测速：并发对全组每个节点调 `/proxies/{name}/delay`，测完一个即经 Channel 推送，前端增量更新）
- **模式切换**：`proxy_set_mode`（controller 转发，切模式后回写 run_params 防重启回退；含「未变跳过」守卫 + emit 同步前端）
- **软重启**：`proxy_reconnect`（免提权软重启：热重载 active config 重建 TUN 栈/连接池，出站异常时一键恢复）
- **诊断流**：`proxy_traffic_stream` / `proxy_connections_stream` / `proxy_logs_stream`（开 WS 流，Channel 推流量速率/连接快照/日志行；mihomo 未运行时静默返回不 spawn，前端三子视图分别显示「无活动连接 / 无规则 / 无日志」）
- **诊断控制**：`proxy_stop_stream`（StreamRegistry CancellationToken 停指定流）/ `proxy_get_rules`（GET /rules 只读快照，未运行返回空）

## 文件布局

`~/Library/Application Support/<bundle-id>/extensions/proxy/`：

- **`config.json`** —— 扩展配置（mode/mixedPort/controllerPort/secret/subscriptions/activeSubscriptionId）
- **`mihomo`** —— 运行时下载的核心 binary
- **`mihomo.version`** —— 已下载 binary 版本号（check_update 比对依据；update_core 删除触发重下）
- **`config.yaml`** —— mihomo 启动配置（**恒 idle**，永不含 TUN 段——崩溃后 launchd 重启只加载 idle，避免循环崩溃）
- **`config-active.yaml`** —— active 运行配置（含 TUN 段），仅经 PUT /configs 热重载加载，不作为启动配置
- **`subs/<id>.yaml`** —— 各订阅原始 Clash YAML
- **`mihomo.log`** —— mihomo 运行日志（launchd 接管 stdout/stderr 写入，启动失败可查）。info 级别每连接一行且 launchd 只追加，**超 5MB 在 stop_core 截断**（O_APPEND 继续写新 EOF，免重启）；诊断读日志走 **64KB 尾读窗口**（`read_log_tail`），不随日志体积放大读取内存
- **`mihomo-daemon.plist`** —— LaunchDaemon plist 临时副本（install 时生成，提权 cat 到 `/Library/LaunchDaemons/`）
- **`geoip.metadb` / `geosite.dat`** —— Geo 数据库（首次使用经 gh-proxy 镜像预下载，mihomo 加载含 GEOIP/GEOSITE 规则的 config 时需此文件）

## 限制

- **提权**：launchd LaunchDaemon 托管，首次开代理 `install_launchdaemon` 提权一次安装 plist，之后开机自启 + 崩溃自愈 + 开关热重载，日常永久零密码框；仅 binary 升级/卸载（`uninstall_launchdaemon` bootout）再提权
- **关闭可靠性**：关代理走热重载 idle config（撤销 TUN + 直通），进程保留不杀（launchd 托管）。controller 卡死时乐观返回成功（不阻塞用户关闭开关，config.yaml 已写 idle 保证 mihomo 重启后直通），后台异步重试释放 TUN，全部失败才 toast。用户可 `proxy_reconnect` 重试
- **进程常驻**：mihomo 由 launchd 托管永久常驻（idle ~50MB，不代理流量，用户无感；idle 无 tun 段不占 TUN）。app 退出不影响（launchd 跨 app 生命周期保活）；重启后 `reconnect_root_mihomo` 验 secret 复用——运行 config 仍 active 则**恢复 enabled 并同步 UI**（代理延续，不静默切直连），idle 则复位直通。secret 不匹配（旧残留）不提权清理，下次开代理 install 接管
- **端口占用**：mihomo 常驻占 mixed-port/controller 端口（idle 也占）；idle 不占 TUN 故 TUN 层可与其他代理软件共存，但端口相同时冲突——install 前端口探测会拦截并提示用户先关闭别的工具或改端口
- **TUN 冲突**：开代理时（idle→active 切换）预检 `netstat -rn` 的 TUN auto-route 路由（老版 `0/1` + `128/1` 半路由 + 新版路由树分解两代风格）——占用者是 Voidnix 对端变体（dev/prod）时自动让渡接管（见上）；其他代理工具已开 TUN 时直接拒绝（明确提示「请先关闭它」），不盲目尝试致 mihomo 残留 broken active config。预检漏过时同步 verify 兜底（检测到 TUN/route error 即回滚 idle config）。**不要同时开两个 TUN 代理工具**（Voidnix 自家两个变体间除外，自动让渡）
