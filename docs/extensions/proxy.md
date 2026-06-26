# proxy

Clash 风格代理扩展。运行时按需下载 mihomo（Clash.Meta）核心，扩展负责进程托管 + 订阅解析 + mihomo controller 转发 + macOS 系统代理。

## 架构

```
前端（Vue，单主界面三 tab）          Rust（native/）
View.vue
 ├ 节点 tab：列表/切换/测速 ──invoke──→ controller.rs ──reqwest──→ mihomo controller API
 ├ 订阅 tab：导入/更新/删除 ──invoke──→ subscription.rs 拉取+解析+合并 Clash YAML
 └ 设置 tab：系统代理/TUN 开关        core.rs ManagedChild 托管 + binary 写盘
                                      system_proxy.rs networksetup 系统代理
                                      tun.rs osascript 提权 root 启动
```

mihomo 监听 `mixed-port`（HTTP+SOCKS5 共用）与 `external-controller`（RESTful API，bearer secret 鉴权）。扩展不解析代理协议，proxies/proxy-groups/rules 原样合并自订阅 Clash YAML。无独立设置子视图，全部功能内联在主界面 tab 中（节点 / 订阅 / 设置）。

## mihomo binary 下载（运行时按需）

不编译期嵌入（避免 Voidnix 二进制膨胀 + 可热更新），改为运行时按需下载。`core::ensure_bin`：binary 已存在即复用（不校验版本，避免无谓重下），否则 `download_core_async` reqwest 流式拉取（经国内镜像 `gh-proxy.com` 前缀，按 `content-length` 推送 `proxy-core-progress` 百分比事件）→ sha256 校验（sha2）→ `gunzip` 解压 → chmod 0o755。镜像仅代理转发，sha256 保证内容一致。代价：首次使用需联网。

升级 mihomo：改 `core.rs` 的 `MIHOMO_VERSION` + `SHA256_ARM64`/`SHA256_AMD64` 常量 + 手动删除 `mihomo` 文件触发重下（binary 存在即复用，不自动按版本替换）。

## 运行模式（二选一，由 config.tunMode 决定）

- **user 模式**（默认）：`core::spawn` 以当前用户启动，`ManagedChild` 托管（Drop kill+wait）。配系统代理（networksetup）让应用层走代理。
- **TUN 模式**：`tun::spawn_root` 经 `osascript do shell script "..." with administrator privileges` 以 root 后台启动（TUN 需 root 创建虚拟网卡 + auto-route），无 Child 句柄，PID 记 `mihomo.pid`，停止走 `tun::stop_root`（提权 kill）。config.yaml 含 `tun`（gvisor stack + dns-hijack + auto-route）+ `dns`（fake-ip）段。切换模式 = 停旧实例 + 以新模式重启。

## 订阅合并（subscription.rs）

`merge_yaml(texts, params)`（纯函数）：多订阅 proxies 按 name 去重拼接；proxy-groups/rules 取首个非空订阅，否则自动生成（`🚀 节点选择` select + `♻️ 自动选择` url-test + `MATCH,🚀 节点选择`）。订阅原文存 `subs/<id>.yaml`，`build_run_config` 启动时读取合并。

订阅拉取走 `http::client()`（SSRF 校验 + Clash UA `clash.meta/v1.19.27`，确保机场返回 YAML 而非 Base64）。增删订阅触发 `restart_if_running` 热重启（运行中即时生效）。

## mihomo controller 转发（controller.rs）

独立 reqwest 客户端（不经 `http::client()` 的 SSRF 防护——controller 固定 127.0.0.1 本地回环）。路径段经 `urlencoding::encode`（支持 emoji 分组名）。端点：`GET /proxies`、`PUT /proxies/{group}`（选节点）、`GET /proxies/{name}/delay`（测速，失败返 0）、`PATCH /configs`（切模式）。

## 系统代理（system_proxy.rs）

`networksetup` 枚举活跃网络服务 → 对每服务设/清 web/secureweb/socksfirewall 三类代理指向 `127.0.0.1:mixedPort`。**macOS GUI 会话下无需 root**（探测验证）。`system_proxy_active` 标记确保仅清除本扩展设过的，不误伤用户其它代理配置；关闭核心时据此 best-effort 清除，防指向已停核心断网。

## 命令（12 个）

`set_proxy_enabled` / `is_proxy_enabled`（启停，含 tun 分支）、`proxy_update_subscription` / `proxy_remove_subscription`（订阅 + 热重启）、`proxy_get_proxies` / `proxy_select_proxy` / `proxy_test_delay` / `proxy_set_mode`（controller 转发）、`proxy_set_system_proxy`、`proxy_enable_tun`（模式切换重启）。

## 文件布局

`~/Library/Application Support/<bundle-id>/extensions/proxy/`：

- `config.json` —— 扩展配置（systemProxy/mode/端口/secret/autoStart/activeSelections/subscriptions）
- `mihomo` —— 运行时下载的核心 binary
- `mihomo.version` —— 已下载 binary 版本号（缓存命中判断）
- `config.yaml` —— mihomo 运行配置（启动时生成）
- `subs/<id>.yaml` —— 各订阅原始 Clash YAML
- `mihomo.pid` —— TUN 模式 root 进程 PID

## 限制

- TUN 提权每次弹系统密码框（osascript 限制；helper LaunchDaemon 工程重，未做）
- panic=abort 下 mihomo 可能残留（user 模式 Drop 不跑、TUN root 进程独立）；首期接受，可加启动期 cleanup 兜底
- 无连接列表/流量统计（mihomo `/connections`/`/traffic` 未接，后续可加）
