# proxy

Clash 风格代理扩展（统一 TUN 模式）。运行时按需下载 mihomo（Clash.Meta）核心，扩展负责 root 进程托管 + 订阅解析 + mihomo controller 转发 + 热重载 active/idle 切换。TUN 由虚拟网卡接管全部 IP 流量（含命令行/Docker/原生应用），是完整的全局代理。

## 架构

```
前端（Vue，单主界面分组列表）        Rust（native/）
View.vue
  ├ 代理组：开启/模式    ──invoke──→ core.rs binary 写盘 + 版本查询
  ├ 订阅组：导入/更新/删除 ──invoke──→ subscription.rs 拉取+解析+合并 Clash YAML
  └ 节点组：列表/切换/测速/分组切 ──invoke──→ controller.rs ──reqwest──→ mihomo controller API
                                       tun.rs osascript 提权 root 启动/停止
```

mihomo 监听 `mixed-port`（HTTP+SOCKS5 共用）与 `external-controller`（RESTful API，bearer secret 鉴权）。扩展不解析代理协议，proxies/proxy-groups/rules 原样合并自订阅 Clash YAML。无独立设置子视图，全部功能内联在主界面的三个分组中（代理 / 订阅 / 节点）；代理分组含开启/规则模式两项。多 selector 分组订阅时节点组首项出现分组切换器。菜单栏图标仅在已连接时显示（打开扩展 + 已连接状态可点断开）；断开隐藏，其余控制全部在面板。

## mihomo binary 下载（运行时按需）

不编译期嵌入（避免 Voidnix 二进制膨胀 + 可热更新），改为运行时按需下载。`core::ensure_bin`：binary 已存在即复用（不校验版本，避免无谓重下），否则下载（并发触发时——双击/开代理 spawn——tokio Mutex + double-check 串行化，仅一个真正下载、其余抢锁后复用，防多流写同一 `mihomo.gz` 损坏致 sha256 失败、binary 无法产出），`download_core_async` 经专用 `http::download_client`（无整体超时，仅建连 30s——慢网络下 15MB gz 下载耗时不可控，全局 `HTTP_CLIENT` 的 120s 整体超时含 body 读取会中途掐断流）reqwest 流式拉取（国内镜像 `gh-proxy.com` 前缀），逐 chunk 推送 `proxy-core-progress` 事件，payload `{ received, total }`：前端按钮文案 `连接中 → N% → 解压中`（未收首字节=连接中；有 Content-Length 且未收齐=N%；字节收齐后 Rust 再 emit 一次 `total=Some(received)` 完成信号=解压中，覆盖 sha256+gunzip 后处理阶段），镜像回源 chunked（`total=None`，实测 gh-proxy 透传 content-length 不触发）时诚实回退显示已收字节。→ sha256 校验（sha2）→ `gunzip` 解压 → chmod 0o755。镜像仅代理转发，sha256 保证内容一致。代价：首次使用需联网。

版本源 + 升级：`fetch_latest_asset` 直连 `api.github.com/repos/MetaCubeX/mihomo/releases/latest` 单次调用拿 `tag_name`（版本）+ `assets[*].digest`（内嵌 sha256，无需独立 digest 文件），asset 名精确串等 `mihomo-darwin-{arch}-{tag}.gz` 排除 go120/go122/go124 变体。API 不可达时 `fallback_asset` 用常量（`MIHOMO_VERSION` + `SHA256_ARM64`/`SHA256_AMD64`）拼装，首次下载仍可用（仅无法获取新版）—— gh-proxy 仅代理 release 下载、不转发 API。升级走 UI：进面板时 `proxy_check_update` 后台拉 latest 比对本地 `mihomo.version`，有新版时副标题显示「内核 vX.Y.Z（最新 vZ，可更新）」+ 出现「更新」按钮。点击触发 `proxy_update_core`：`stop_root`（若在跑）→ 删 binary + version → `ensure_bin` 重下最新 → `start_core` 恢复（若之前 enabled）。中途下载失败：文件已删，下次 `ensure_bin` 自动重试。

下载状态真相源在 Rust：`DOWNLOADING` 原子标记下载中，`core_status` 据此返回。前端 `isDownloading` = `coreStatus.downloading`（computed），重新进入界面 `loadCoreStatus` 反映下载中/已下载/未下载。进度数值不持久化（下载是低频一次性操作，不值得为之维护快照）——退出重进时若仍在下载，按钮显示「下载中」，收到下一个 `proxy-core-progress` 事件即恢复具体百分比。gunzip/chmod/version 全部就绪后 Rust emit `proxy-core-ready`，前端事件驱动 `loadCoreStatus` 刷新——不依赖 `invoke(proxyEnsureCore)` 的 resolve 时序（sha256/gunzip 同步阻塞可能延迟 IPC 响应，曾导致前端乐观标记的 `downloading=true` 不被刷新、UI 卡在「解压中」）。下载与启用解耦：`downloadCore` 不自动 `toggleEnabled`，内核就绪后用户手动开启。

Geo 数据库（`ensure_geo_files`）：mihomo 加载含 GEOIP/GEOSITE 规则的 config 时需 `geoip.metadb` + `geosite.dat`，缺失触发同步下载（直连 GitHub 国内不可达 → EOF → config 加载失败/控制器不启动 → 开代理超时）。`restart_root` 前置 `ensure_geo_files` 经 gh-proxy 镜像预下载（已存在跳过，失败不阻塞——mihomo 经 `geox-url` 自行重试）。

## 运行模式（统一 TUN）

mihomo 以 root 经 `tun::restart_root`（`osascript do shell script "..." with administrator privileges`）后台启动——TUN 需 root 创建虚拟网卡 + auto-route，接管全部 IP 流量。**TUN 是系统独占资源**（虚拟网卡 + 路由 `1.0.0.0/8` 等），两个 mihomo 实例不能同时占 TUN（`add route: file exists`），故 `restart_root` 启动前先清理**所有 Voidnix mihomo**（按 `com.litiantao.voidnix` 前缀 ps 匹配，覆盖 dev/prod 两个 bundle id 数据目录路径）释放 TUN + 端口，再启动新的。无 Child 句柄，PID 记 `mihomo.pid`，停止走 `tun::stop_root`（提权：SIGTERM → 轮询 `kill -0` 确认 → SIGKILL 兜底 → 按 binary 路径扫杀孤儿 → 验证确死；仅杀自己路径的，关代理不影响另一版）。config.yaml 含 `tun`（gvisor stack + dns-hijack + auto-route）+ `dns`（fake-ip）段。`run_osascript` 调用期间 `click_monitor::suppress`——SecurityAgent 授权框在主窗口外，用户点击（输密码/确认）会被判为 click-outside 触发 hideWindow 致窗口提前关闭。

### root mihomo 常驻 + 热重载 active/idle（免反复提权）

osascript 每次提权都弹系统密码框，若「开/关代理 = spawn/kill root 进程」则每次开关 2 次密码。改为**进程生命周期与流量开关解耦**：

- **首次开代理**：`ensure_root_mihomo` restart_root（提权 1 次，清理所有 Voidnix mihomo + 启新）+ 热重载 active config；之后 `tun_active=true`，进程常驻
- **关代理**：`stop_core` 优先热重载 idle config（`mode=direct + 无 tun 段`）→ mihomo 撤销 utun、流量直通（被墙不可达，符合「关闭」语义），**进程保留**。热重载失败（mihomo 崩溃/controller 卡死致 127.0.0.1:9090 不可达）回退强杀保关闭可靠性：进程已退出则直接重置状态（TUN 已由 OS 回收，免提权），进程仍跑（卡死）则 `stop_root` 强杀释放 TUN
- **再开代理**：`start_core` 检测 `tun_active=true` → 热重载 active config 恢复，**免提权**；热重载失败（罕见边缘情况）时回退 `restart_root` 重启进程

效果：密码从「每次开关 2 次」→「首次 1 次，之后 0 次」。代价：root mihomo 进程首次开代理后常驻到 app 退出（idle ~50MB，不代理流量，用户无感）。app 启动时 `reconnect_root_mihomo` 检测上次遗留的 root mihomo（按 binary 路径 ps 查）：先 `GET /version` 验 secret——匹配则热重载 idle（重置为 direct 直通，idle config 复用真实 mixed_port 与 `stop_core` 一致），成功才标记 `tun_active`（复用），reload 因配置/瞬态失败不杀进程（保留常驻免提权）；secret 不匹配（401 = 不可控）即 `stop_root` 清理僵尸。

`ensure_root_mihomo` 仅信任 `tun_active=true` 的进程（reconnect 成功置 idle 或本 session spawn）可热重载复用；`tun_active=false` → 状态不可信（secret 可能不匹配、TUN 可能已被其他实例占用），`restart_root` 单次 osascript 提权完成**杀所有 Voidnix mihomo**+启新（SIGTERM→轮询至多 2s 确认退出→SIGKILL→sleep 1 等端口/utun 释放→spawn）。按 `com.litiantao.voidnix` 前缀匹配 dev/prod 两版数据目录路径——TUN 互斥决定了同一时刻只能有一个 Voidnix mihomo 占 TUN，开代理即接管。mihomo 输出重定向到 mihomo.log（启动失败可查日志）。

> 历史上有过 user 模式（系统代理，仅覆盖遵守系统代理的应用）+ TUN 模式双选；因 user 模式覆盖不全（命令行/Docker/部分原生应用不走代理）非完整代理，已移除，统一为 TUN。

## 健康监测 + 自动热重载恢复

root mihomo 常驻但进程可能因出站失效/接口抖动/异常退出而「假死」——内存状态（enabled/tun_active）与实际脱节：UI 仍显示已开启，用户靠测速才发现全超时；关闭重开还因热重载失败走 stop_root + restart_root 双提权。

`start_core` 成功后 spawn 健康监测 task（`ensure_monitor`，幂等）：每 30s 探针（`probe_health` = controller GET /version 可达 + 当前主 selector 节点 delay test 出站可达），连续 2 轮异常（容忍单次抖动）才动作：

- 进程已退出（`!root_mihomo_running`）或 controller 不可达 → `reset_dead_state`：重置 enabled/tun_active/monitor_alive + 清残留 pidfile + emit `proxy-enabled:false` + emit `proxy-status{kind:error}`（前端状态栏提醒 + 开启项红色提示）+ menubar 隐藏图标。**不自动提权重启**（避免突兀弹密码框），由用户手动重新开启
- 进程在 + controller 在 但出站死 → **免提权热重载 active config**（`reload_config_yaml`，PUT /configs 让 mihomo 重建 gvisor/连接池/接口绑定，对症「重启就好」）；热重载失败 emit `proxy-status{kind:error}` 通知，下轮重试

用户亦可手动触发 `proxy_reconnect`（免提权软重启，UI 开启项「重连」按钮）：进程在 + controller 可达时一键热重载 active config，规避关闭→开启的 stop_root 提权。进程已退出则报错提示需关闭重开（会提权）。`stop_core` 置 monitor_alive=false 停监测（用户主动关闭无需监测）。

## 订阅合并（subscription.rs）

`merge_yaml(texts, params)`（纯函数）：多订阅 proxies 按 name 去重拼接；proxy-groups/rules 取首个非空订阅，否则自动生成（`节点选择` select + `自动选择` url-test + `MATCH,节点选择`）。config 含 `geox-url`（geoip/geosite 镜像 URL，国内直连 GitHub 不可达致 mihomo 下载 EOF）。DNS 配置：nameserver 国内直连（223.5.5.5/119.29.29.29，fake-ip 查询 + DIRECT 流量真实解析均走此，国内 DNS 对常见海外域名如 apple.com 返回正确 IP）+ `proxy-server-nameserver`（节点域名专用，保证 TUN 排除路由可靠添加防回环）。**不配 fallback/fallback-filter**：海外 DoH 在 TUN 下经代理，DIRECT 海外域名解析会串行等待 fallback（实测 apple.com couldn't find ip），是 active 比 idle 测速慢一个数量级的根因；被污染域名走代理远程解析，不依赖本地 DNS。订阅原文存 `subs/<id>.yaml`，`build_run_config` 启动时读取合并。

订阅拉取走 `http::client()`（SSRF 校验 + Clash UA `clash.meta/v1.19.27`，确保机场返回 YAML 而非 Base64）。增删订阅触发 `reload_if_running`：重建 `config.yaml` 后 `PUT /configs {path}` 让 mihomo 原生热重载（root 进程常驻，免重启免再提权）。

## mihomo controller 转发（controller.rs）

独立 reqwest 客户端（不经 `http::client()` 的 SSRF 防护——controller 固定 127.0.0.1 本地回环）。路径段经 `urlencoding::encode`（支持 emoji 分组名）。端点：`GET /proxies`、`PUT /proxies/{group}`（选节点）、`GET /proxies/{name}/delay`（测速，失败返 0）、`PATCH /configs`（切模式）、`PUT /configs {path}`（热重载配置）。

## 聚合菜单栏贡献（mod.rs）

代理已连接时向框架统一菜单栏托盘（`runtime/menubar.rs`，`public/bar_icon.png` 模板图）贡献两项——极简 + 唯一（控制逻辑全部在扩展面板，菜单不重复）；断开后 `build` 返回空，图标自动隐藏。`setup` 内 `menubar::register` 声明 `build`/`on_event`，状态变更后 `menubar::refresh` 重建。

- **打开扩展**（Item，点击打开代理面板）+ **已连接：节点**（CheckItem 勾选，点击断开 → 图标隐藏，重连走扩展面板）

状态行当前节点名由 `refresh_proxy_menu` 异步拉 `controller::get_proxies` → `parse_current_node`（取主 selector 的 `now`）填充缓存（`ProxyState.current_node`）；`set_proxy_enabled` / `proxy_select_proxy` / `reload_if_running` 触发刷新。点击状态行调 `stop_core` 热重载 idle 断开代理，emit `proxy-enabled:false` 同步面板 + refresh 使 `build` 返回空 → 图标隐藏。其余控制（模式/订阅/节点切换/测速）仍在扩展面板。

## 命令（11 个）

`set_proxy_enabled` / `is_proxy_enabled`（启停；root mihomo 常驻——首次 restart_root 提权一次，之后开关走热重载 active/idle config 免提权）、`proxy_core_status` / `proxy_ensure_core`（内核版本查询与运行时按需下载）、`proxy_check_update` / `proxy_update_core`（拉 GitHub API latest 比对版本 / 停代理 + 删旧 + 重下 + 恢复）、`proxy_update_subscription` / `proxy_remove_subscription`（订阅 + 热重载）、`proxy_get_proxies` / `proxy_select_proxy` / `proxy_test_delay` / `proxy_set_mode`（controller 转发，切模式后回写 run_params 防重启回退）、`proxy_reconnect`（免提权软重启：热重载 active config 重建 gvisor/连接池，出站异常时一键恢复，规避关闭→开启的 stop_root 提权）。`proxy_set_mode` 含「未变跳过」守卫 + emit 同步前端。

## 文件布局

`~/Library/Application Support/<bundle-id>/extensions/proxy/`：

- `config.json` —— 扩展配置（mode/mixedPort/controllerPort/secret/subscriptions）
- `mihomo` —— 运行时下载的核心 binary
- `mihomo.version` —— 已下载 binary 版本号（check_update 比对依据；update_core 删除触发重下）
- `config.yaml` —— mihomo 运行配置（active/idle 切换时重写）
- `subs/<id>.yaml` —— 各订阅原始 Clash YAML
- `mihomo.pid` —— root 进程 PID
- `mihomo.log` —— mihomo 运行日志（每次 restart 截断重写，启动失败可查）
- `geoip.metadb` / `geosite.dat` —— Geo 数据库（首次使用经 gh-proxy 镜像预下载，mihomo 加载含 GEOIP/GEOSITE 规则的 config 时需此文件）

## 限制

- 提权：osascript 限制无法完全免密，但 root mihomo 常驻方案将密码从「每次开关 2 次」降至「首次开代理 1 次，之后 0 次」（彻底停止/app 退出后重启需再次提权）。SMJobBless helper 可实现首次安装后全程免密，但 Rust 无成熟 XPC server 绑定 + 签名/打包工程重，未做
- 关闭可靠性：`stop_root` 按 pidfile PID 优雅停（SIGTERM → 轮询 `kill -0` 至 3s → SIGKILL），再按 mihomo binary 完整路径（含 bundle-id 数据目录）扫杀自己路径的所有残留进程（防 pidfile 脱节、多次 restart 累积孤儿；不杀另一版 Voidnix mihomo），末尾验证确死否则报错
- 进程常驻：root mihomo 首次开代理后常驻到 app 退出（idle ~50MB，不代理流量，用户无感；无主动停止入口，靠 app 退出 + reconnect 复用）。app 退出后进程仍跑，下次启动 `reconnect_root_mihomo` 先验 secret（GET /version）：匹配则热重载 idle 重置状态（防端口冲突 + 状态一致），不匹配（401）则 stop_root 清理僵尸。panic=abort 下无 Drop 清理，仅崩溃场景可能残留，启动期 reconnect 兜底
- 无连接列表/流量统计（mihomo `/connections`/`/traffic` 未接，后续可加）
