# proxy

Clash 风格代理扩展。运行时按需下载 mihomo（Clash.Meta）核心，扩展负责进程托管 + 订阅解析 + mihomo controller 转发 + macOS 系统代理。

## 架构

```
前端（Vue，单主界面分组列表）        Rust（native/）
View.vue
  ├ 代理组：开启/TUN/模式  ──invoke──→ core.rs ManagedChild 托管 + binary 写盘
  │                                  （user 模式启用自动设系统代理，TUN 模式免）
  ├ 订阅组：导入/更新/删除         ──invoke──→ subscription.rs 拉取+解析+合并 Clash YAML
  └ 节点组：列表/切换/测速/分组切   ──invoke──→ controller.rs ──reqwest──→ mihomo controller API
                                       system_proxy.rs networksetup 系统代理
                                       tun.rs osascript 提权 root 启动
```

mihomo 监听 `mixed-port`（HTTP+SOCKS5 共用）与 `external-controller`（RESTful API，bearer secret 鉴权）。扩展不解析代理协议，proxies/proxy-groups/rules 原样合并自订阅 Clash YAML。无独立设置子视图，全部功能内联在主界面的三个分组中（代理 / 订阅 / 节点）；代理分组含开启/TUN/规则模式三项（系统代理为 user 模式隐含行为，不再独立）。多 selector 分组订阅时节点组首项出现分组切换器。代理开启后向聚合菜单栏贡献三项（见「聚合菜单栏贡献」段），不打开面板也能开关 / 切节点。

## mihomo binary 下载（运行时按需）

不编译期嵌入（避免 Voidnix 二进制膨胀 + 可热更新），改为运行时按需下载。`core::ensure_bin`：binary 已存在即复用（不校验版本，避免无谓重下），否则 `download_core_async` reqwest 流式拉取（经国内镜像 `gh-proxy.com` 前缀，按 `content-length` 推送 `proxy-core-progress` 百分比事件）→ sha256 校验（sha2）→ `gunzip` 解压 → chmod 0o755。镜像仅代理转发，sha256 保证内容一致。代价：首次使用需联网。

升级 mihomo：改 `core.rs` 的 `MIHOMO_VERSION` + `SHA256_ARM64`/`SHA256_AMD64` 常量 + 手动删除 `mihomo` 文件触发重下（binary 存在即复用，不自动按版本替换）。

## 运行模式（二选一，由 config.tunMode 决定）

- **user 模式**（默认）：`core::spawn` 以当前用户启动，`ManagedChild` 托管（Drop kill+wait）。配系统代理（networksetup）让应用层走代理。
- **TUN 模式**：`tun::spawn_root` 经 `osascript do shell script "..." with administrator privileges` 以 root 后台启动（TUN 需 root 创建虚拟网卡 + auto-route），无 Child 句柄，PID 记 `mihomo.pid`，停止走 `tun::stop_root`（提权 kill）。config.yaml 含 `tun`（gvisor stack + dns-hijack + auto-route）+ `dns`（fake-ip）段。切换模式 = 停旧实例 + 以新模式重启。

## 订阅合并（subscription.rs）

`merge_yaml(texts, params)`（纯函数）：多订阅 proxies 按 name 去重拼接；proxy-groups/rules 取首个非空订阅，否则自动生成（`🚀 节点选择` select + `♻️ 自动选择` url-test + `MATCH,🚀 节点选择`）。订阅原文存 `subs/<id>.yaml`，`build_run_config` 启动时读取合并。

订阅拉取走 `http::client()`（SSRF 校验 + Clash UA `clash.meta/v1.19.27`，确保机场返回 YAML 而非 Base64）。增删订阅触发 `reload_if_running`：重建 `config.yaml` 后 `PUT /configs {path}` 让 mihomo 原生热重载（user/root 两种模式统一生效，免去进程重启及 TUN 模式下重启会漏停 root 实例的死局）。

## mihomo controller 转发（controller.rs）

独立 reqwest 客户端（不经 `http::client()` 的 SSRF 防护——controller 固定 127.0.0.1 本地回环）。路径段经 `urlencoding::encode`（支持 emoji 分组名）。端点：`GET /proxies`、`PUT /proxies/{group}`（选节点）、`GET /proxies/{name}/delay`（测速，失败返 0）、`PATCH /configs`（切模式）、`PUT /configs {path}`（热重载配置）。

## 系统代理（system_proxy.rs）

`networksetup` 枚举活跃网络服务 → 对每服务设/清 web/secureweb/socksfirewall 三类代理指向 `127.0.0.1:mixedPort`。**macOS GUI 会话下无需 root**（探测验证）。`system_proxy_active` 标记确保仅清除本扩展设过的，不误伤用户其它代理配置；关闭核心时据此 best-effort 清除，防指向已停核心断网。

系统代理不再暴露为独立开关——降级为运行模式的派生行为：user 模式启用核心时自动 `apply` 设上（`set_proxy_enabled` 内），切到 TUN 时清除（TUN 虚拟网卡已接管全部流量），切回 user 时重设（`apply_tun` 内同步）。用户只需操作「开启代理」与「TUN 模式」两项。

## 聚合菜单栏贡献（mod.rs）

代理开启后向框架统一菜单栏托盘（`runtime/menubar.rs`，`public/bar_icon.png` 模板图）贡献菜单项（镜像界面「代理」分组文案）；停用时 `build` 返回空不再贡献（扩展全关则图标自动隐藏）。`setup` 内 `menubar::register` 声明 `build`/`on_event`，状态变更后 `menubar::refresh` 重建。

菜单结构（`build` 同步读 `enabled`/`tun_active`/`run_params.mode` + `menu_subs`/`menu_nodes` 缓存；节点缓存由 `refresh_proxy_menu` 异步拉 `controller::get_proxies` → `parse_main_group` 填充，订阅名由前端 `proxy_sync_menu_subs` 推送）：

- **开启代理**（CheckItem，恒勾选）→ `stop_core` 关闭（图标随之隐藏）
- **TUN 模式**（CheckItem，反映 `tun_active`）→ `apply_tun` 翻转
- **规则模式 ▸**（子菜单：规则/全局/直连，反映 `run_params.mode`）→ `proxy_set_mode` 切换
- **订阅 ▸**（子菜单：各订阅名）→ 点击打开代理面板
- **节点 ▸**（主分组节点 CheckItem，标当前选中）→ `controller::select_proxy`

菜单操作均经命令路径，命令结尾 emit 对应事件（`proxy-enabled`/`proxy-tun`/`proxy-mode`）同步前端 config/isEnabled；命令内含「未变跳过」守卫防前端 watch 回声（菜单改 → emit → 前端更新 config → watch 再推 → 守卫跳过）。enabled / 节点列表仍由面板 `onMounted` 的 `checkStatus` / `loadProxies` 首读。

## 命令（12 个）

`set_proxy_enabled` / `is_proxy_enabled`（启停，含 tun 分支；user 模式启用自动设系统代理）、`proxy_core_status` / `proxy_ensure_core`（内核版本查询与运行时按需下载）、`proxy_update_subscription` / `proxy_remove_subscription`（订阅 + 热重载）、`proxy_get_proxies` / `proxy_select_proxy` / `proxy_test_delay` / `proxy_set_mode`（controller 转发，切模式后回写 run_params 防重启回退）、`proxy_enable_tun`（首次提权切 root；root 运行后热重载 config 免重启免提权；切换时同步系统代理：TUN 清、user 重设）、`proxy_sync_menu_subs`（前端推送订阅名 → 缓存供菜单子菜单展示）。`proxy_set_mode` 含「未变跳过」守卫 + emit 同步前端。

## 文件布局

`~/Library/Application Support/<bundle-id>/extensions/proxy/`：

- `config.json` —— 扩展配置（tunMode/mode/mixedPort/controllerPort/secret/subscriptions）
- `mihomo` —— 运行时下载的核心 binary
- `mihomo.version` —— 已下载 binary 版本号（缓存命中判断）
- `config.yaml` —— mihomo 运行配置（启动时生成）
- `subs/<id>.yaml` —— 各订阅原始 Clash YAML
- `mihomo.pid` —— TUN 模式 root 进程 PID

## 限制

- TUN 提权每次弹系统密码框（osascript 限制；helper LaunchDaemon 工程重，未做）
- panic=abort 下 user 模式 `ManagedChild::drop` 被跳过、TUN root 进程独立——正常退出 Drop 会 `kill+wait`，仅崩溃场景可能残留，可加启动期 cleanup 兜底
- 无连接列表/流量统计（mihomo `/connections`/`/traffic` 未接，后续可加）
