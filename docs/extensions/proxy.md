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

mihomo 监听 `mixed-port`（HTTP+SOCKS5 共用）与 `external-controller`（RESTful API，bearer secret 鉴权）。扩展不解析代理协议，proxies/proxy-groups/rules 原样合并自订阅 Clash YAML。无独立设置子视图，全部功能内联在主界面的三个分组中（代理 / 订阅 / 节点）；代理分组含开启/规则模式两项。多 selector 分组订阅时节点组首项出现分组切换器。代理开启后向聚合菜单栏贡献菜单（见「聚合菜单栏贡献」段），不打开面板也能开关 / 切节点。

## mihomo binary 下载（运行时按需）

不编译期嵌入（避免 Voidnix 二进制膨胀 + 可热更新），改为运行时按需下载。`core::ensure_bin`：binary 已存在即复用（不校验版本，避免无谓重下），否则下载（并发触发时——双击/开代理 spawn——tokio Mutex + double-check 串行化，仅一个真正下载、其余抢锁后复用，防多流写同一 `mihomo.gz` 损坏致 sha256 失败、binary 无法产出），`download_core_async` 经专用 `http::download_client`（无整体超时，仅建连 30s——慢网络下 15MB gz 下载耗时不可控，全局 `HTTP_CLIENT` 的 120s 整体超时含 body 读取会中途掐断流）reqwest 流式拉取（国内镜像 `gh-proxy.com` 前缀），逐 chunk 推送 `proxy-core-progress` 事件，payload `{ received, total }`：前端按钮文案 `连接中 → N% → 解压中`（未收首字节=连接中；有 Content-Length 且未收齐=N%；字节收齐后 Rust 再 emit 一次 `total=Some(received)` 完成信号=解压中，覆盖 sha256+gunzip 后处理阶段），镜像回源 chunked（`total=None`，实测 gh-proxy 透传 content-length 不触发）时诚实回退显示已收字节。→ sha256 校验（sha2）→ `gunzip` 解压 → chmod 0o755。镜像仅代理转发，sha256 保证内容一致。代价：首次使用需联网。

升级 mihomo：改 `core.rs` 的 `MIHOMO_VERSION` + `SHA256_ARM64`/`SHA256_AMD64` 常量 + 手动删除 `mihomo` 文件触发重下（binary 存在即复用，不自动按版本替换）。

## 运行模式（统一 TUN）

mihomo 以 root 经 `tun::spawn_root`（`osascript do shell script "..." with administrator privileges`）后台启动——TUN 需 root 创建虚拟网卡 + auto-route，接管全部 IP 流量。无 Child 句柄，PID 记 `mihomo.pid`，停止走 `tun::stop_root`（提权：SIGTERM → 轮询 `kill -0` 确认 → SIGKILL 兜底 → 按 binary 路径扫杀孤儿 → 验证确死）。config.yaml 含 `tun`（gvisor stack + dns-hijack + auto-route）+ `dns`（fake-ip）段。

### root mihomo 常驻 + 热重载 active/idle（免反复提权）

osascript 每次提权都弹系统密码框，若「开/关代理 = spawn/kill root 进程」则每次开关 2 次密码。改为**进程生命周期与流量开关解耦**：

- **首次开代理**：`ensure_root_mihomo` spawn_root（提权 1 次）+ 热重载 active config；之后 `tun_active=true`，进程常驻
- **关代理**：`stop_core` 不 kill，改热重载 idle config（`mode=direct + 无 tun 段`）→ mihomo 撤销 utun、流量直通（被墙不可达，符合「关闭」语义），**进程保留**
- **再开代理**：`start_core` 检测 `tun_active=true` → 热重载 active config 恢复，**免提权**；热重载失败（残留进程 secret 不一致——mihomo controller 的 `secret` 启动时固化、reload 不生效）时回退 `stop_root` + `spawn_root` 重启进程（用当前 secret）

效果：密码从「每次开关 2 次」→「首次 1 次，之后 0 次」。代价：root mihomo 进程首次开代理后常驻到 app 退出（idle ~50MB，不代理流量，用户无感）。app 启动时 `reconnect_root_mihomo` 检测上次遗留的 root mihomo（按 binary 路径 ps 查）并热重载 idle 重置状态——成功才标记 `tun_active`（复用），失败（secret 不一致等）不标记、留给开代理时回退重 spawn，防端口冲突 + 免重提权。

> 历史上有过 user 模式（系统代理，仅覆盖遵守系统代理的应用）+ TUN 模式双选；因 user 模式覆盖不全（命令行/Docker/部分原生应用不走代理）非完整代理，已移除，统一为 TUN。

## 订阅合并（subscription.rs）

`merge_yaml(texts, params)`（纯函数）：多订阅 proxies 按 name 去重拼接；proxy-groups/rules 取首个非空订阅，否则自动生成（`🚀 节点选择` select + `♻️ 自动选择` url-test + `MATCH,🚀 节点选择`）。订阅原文存 `subs/<id>.yaml`，`build_run_config` 启动时读取合并。

订阅拉取走 `http::client()`（SSRF 校验 + Clash UA `clash.meta/v1.19.27`，确保机场返回 YAML 而非 Base64）。增删订阅触发 `reload_if_running`：重建 `config.yaml` 后 `PUT /configs {path}` 让 mihomo 原生热重载（root 进程常驻，免重启免再提权）。

## mihomo controller 转发（controller.rs）

独立 reqwest 客户端（不经 `http::client()` 的 SSRF 防护——controller 固定 127.0.0.1 本地回环）。路径段经 `urlencoding::encode`（支持 emoji 分组名）。端点：`GET /proxies`、`PUT /proxies/{group}`（选节点）、`GET /proxies/{name}/delay`（测速，失败返 0）、`PATCH /configs`（切模式）、`PUT /configs {path}`（热重载配置）。

## 聚合菜单栏贡献（mod.rs）

代理开启时向框架统一菜单栏托盘（`runtime/menubar.rs`，`public/bar_icon.png` 模板图）贡献菜单项；关闭（含 idle 常驻）时 `build` 返回空，图标自动隐藏——保持菜单栏干净。`setup` 内 `menubar::register` 声明 `build`/`on_event`，状态变更后 `menubar::refresh` 重建。

菜单结构（`build` 同步读 `run_params.mode` + `menu_subs`/`menu_nodes` 缓存；节点缓存由 `refresh_proxy_menu` 异步拉 `controller::get_proxies` → `parse_main_group` 填充，订阅名由前端 `proxy_sync_menu_subs` 推送）：

- **开启代理**（CheckItem 勾选，点击 = 关闭转 idle）+ **规则模式 ▸**（规则/全局/直连）+ **订阅 ▸** + **节点 ▸**

菜单操作均经命令路径，命令结尾 emit 对应事件（`proxy-enabled`/`proxy-mode`）同步前端 isEnabled/mode；命令内含「未变跳过」守卫防前端 watch 回声。enabled / 节点列表仍由面板 `onMounted` 的 `checkStatus` / `loadProxies` 首读。

## 命令（11 个）

`set_proxy_enabled` / `is_proxy_enabled`（启停；root mihomo 常驻——首次 spawn_root 提权一次，之后开关走热重载 active/idle config 免提权）、`proxy_core_status` / `proxy_ensure_core`（内核版本查询与运行时按需下载）、`proxy_update_subscription` / `proxy_remove_subscription`（订阅 + 热重载）、`proxy_get_proxies` / `proxy_select_proxy` / `proxy_test_delay` / `proxy_set_mode`（controller 转发，切模式后回写 run_params 防重启回退）、`proxy_sync_menu_subs`（前端推送订阅名 → 缓存供菜单子菜单展示）。`proxy_set_mode` 含「未变跳过」守卫 + emit 同步前端。

## 文件布局

`~/Library/Application Support/<bundle-id>/extensions/proxy/`：

- `config.json` —— 扩展配置（mode/mixedPort/controllerPort/secret/subscriptions）
- `mihomo` —— 运行时下载的核心 binary
- `mihomo.version` —— 已下载 binary 版本号（缓存命中判断）
- `config.yaml` —— mihomo 运行配置（active/idle 切换时重写）
- `subs/<id>.yaml` —— 各订阅原始 Clash YAML
- `mihomo.pid` —— root 进程 PID

## 限制

- 提权：osascript 限制无法完全免密，但 root mihomo 常驻方案将密码从「每次开关 2 次」降至「首次开代理 1 次，之后 0 次」（彻底停止/app 退出后重启需再次提权）。SMJobBless helper 可实现首次安装后全程免密，但 Rust 无成熟 XPC server 绑定 + 签名/打包工程重，未做
- 关闭可靠性：`stop_root` 按 pidfile PID 优雅停（SIGTERM → 轮询 `kill -0` 至 3s → SIGKILL），再按 mihomo binary 完整路径（含 bundle-id 数据目录）扫杀所有残留进程（防 pidfile 脱节、多次 spawn 累积孤儿），末尾验证确死否则报错
- 进程常驻：root mihomo 首次开代理后常驻到 app 退出（idle ~50MB，不代理流量，用户无感；无主动停止入口，靠 app 退出 + reconnect 复用）。app 退出后进程仍跑，下次启动 `reconnect_root_mihomo` 检测复用并热重载 idle 重置状态（防端口冲突 + 状态一致）。panic=abort 下无 Drop 清理，仅崩溃场景可能残留，启动期 reconnect 兜底
- 无连接列表/流量统计（mihomo `/connections`/`/traffic` 未接，后续可加）
