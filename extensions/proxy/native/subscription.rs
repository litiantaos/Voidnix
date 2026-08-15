use crate::http;
use crate::runtime::storage::ext_data_dir;
use serde_yml::{Mapping, Value};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;
use tauri::AppHandle;

use super::controller::DELAY_TEST_URL;
use super::core::{RunParams, GEO_RELEASE_BASE, MIRROR_PREFIX};

/// 订阅请求 UA：部分机场据此返回 Clash YAML 而非 Base64 订阅，与 mihomo 版本对齐。
const SUB_UA: &str = "clash.meta/v1.19.27";

fn sub_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = ext_data_dir(app, "proxy")?.join("subs");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// 订阅 id 合法性：非空且不含路径分隔符（id 来自命令参数，防 `../` 逃出 subs/ 目录；
/// 前端自生成 id 不会违规，此为命令边界防御）。
fn valid_sub_id(id: &str) -> bool {
    !id.is_empty() && !id.contains('/') && !id.contains('\\')
}

fn sub_yaml_path(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    if !valid_sub_id(id) {
        return Err(format!("非法订阅 id: {id}"));
    }
    Ok(sub_dir(app)?.join(format!("{id}.yaml")))
}

/// 拉取订阅（SSRF 校验 + Clash UA），返回 (proxy 数, 原始 YAML 文本)。
pub async fn fetch(url: &str) -> Result<(usize, String), String> {
    http::validate_url(url)?;
    let text = http::client()
        .get(url)
        .header(reqwest::header::USER_AGENT, SUB_UA)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| format!("订阅请求失败: {e}"))?
        .error_for_status()
        .map_err(|e| format!("订阅响应错误: {e}"))?
        .text()
        .await
        .map_err(|e| format!("读取订阅失败: {e}"))?;
    let count = count_proxies(&text)?;
    Ok((count, text))
}

/// 解析 YAML 统计 proxies 数量（非法 YAML 报错）。
fn count_proxies(yaml_text: &str) -> Result<usize, String> {
    let val: Value = serde_yml::from_str(yaml_text).map_err(|e| format!("非 Clash YAML: {e}"))?;
    Ok(val
        .get("proxies")
        .and_then(|p| p.as_sequence())
        .map(|s| s.len())
        .unwrap_or(0))
}

/// 保存订阅原始 YAML（下次启动 mihomo 时合并入 config.yaml）。
pub fn save(app: &AppHandle, id: &str, yaml_text: &str) -> Result<(), String> {
    std::fs::write(sub_yaml_path(app, id)?, yaml_text).map_err(|e| e.to_string())
}

/// 删除订阅 YAML。
pub fn remove(app: &AppHandle, id: &str) -> Result<(), String> {
    let p = sub_yaml_path(app, id)?;
    if p.exists() {
        std::fs::remove_file(p).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn s(v: &str) -> Value {
    Value::String(v.to_string())
}

/// 读取激活订阅的原始 YAML（`subs/<active_sub_id>.yaml`）交 merge_yaml 合并生成 config.yaml 文本。
///
/// 单激活模型：同一时刻仅一个订阅生效（前端 config.activeSubscriptionId），未激活订阅的
/// YAML 仅缓存于 `subs/` 待激活，不参与合并——避免多订阅节点/分组混杂，节点列表只呈现激活订阅。
pub fn build_run_config(app: &AppHandle, params: &RunParams) -> Result<String, String> {
    let mut texts: Vec<String> = Vec::new();
    if !params.active_sub_id.is_empty() {
        let path = sub_yaml_path(app, &params.active_sub_id)?;
        if let Ok(t) = std::fs::read_to_string(&path) {
            texts.push(t);
        }
    }
    merge_yaml(&texts, params)
}

/// 合并 Clash YAML 文本 + 基础配置 → config.yaml 文本（纯函数，便于单测）。
///
/// 单激活模型下 texts 通常仅 1 份（激活订阅）；保留多文本合并与按 name 去重作为防御
/// （单订阅内重名节点同样去重）。合并策略：
/// - proxies：所有文本按 name 去重拼接
/// - proxy-groups：取首个含非空 proxy-groups 的文本；否则自动生成 select + url-test
/// - rules：取首个含非空 rules 的文本；否则默认 GEOIP CN 直连 + MATCH 代理
pub fn merge_yaml(texts: &[String], params: &RunParams) -> Result<String, String> {
    let mut all_proxies: Vec<Value> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut groups: Option<Value> = None;
    let mut rules: Option<Value> = None;

    for text in texts {
        let val: Value = match serde_yml::from_str(text) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(proxies) = val.get("proxies").and_then(|p| p.as_sequence()) {
            for p in proxies {
                let name = p
                    .get("name")
                    .and_then(|n| n.as_str())
                    .map(|x| x.to_string());
                if let Some(n) = &name {
                    if !seen.insert(n.clone()) {
                        continue;
                    }
                }
                all_proxies.push(p.clone());
            }
        }
        if groups.is_none() {
            if let Some(g) = val
                .get("proxy-groups")
                .filter(|g| g.as_sequence().map(|s| !s.is_empty()).unwrap_or(false))
            {
                groups = Some(g.clone());
            }
        }
        if rules.is_none() {
            if let Some(r) = val
                .get("rules")
                .filter(|r| r.as_sequence().map(|s| !s.is_empty()).unwrap_or(false))
            {
                rules = Some(r.clone());
            }
        }
    }

    let mut root = Mapping::new();
    root.insert(s("mixed-port"), Value::from(i64::from(params.mixed_port)));
    root.insert(
        s("external-controller"),
        s(&format!("127.0.0.1:{}", params.controller_port)),
    );
    root.insert(s("secret"), s(&params.secret));
    root.insert(s("mode"), s(&params.mode));
    root.insert(s("log-level"), s("info"));
    root.insert(s("allow-lan"), Value::Bool(false));
    // 全局性能开关（mihomo 推荐，对比 Clash Verge Rev / Mihomo Party 等图形客户端默认注入）：
    // unified-delay：测速减去握手耗时（DNS+TCP+TLS+协议握手），只留 HTTP RTT。缺失此项是
    //   节点测速数值偏高一个数量级的根因——ANYTLS 等 TLS 协议握手 300-600ms 被计入延迟。
    // tcp-concurrent：多 IP 节点并发建连取最快（Happy Eyeballs 并行而非串行），降首包延迟。
    // keep-alive-interval：连接保活探测，复用长连接降测速波动与重复握手开销。
    root.insert(s("unified-delay"), Value::Bool(true));
    root.insert(s("tcp-concurrent"), Value::Bool(true));
    root.insert(s("keep-alive-interval"), Value::from(30i64));

    // Geo 数据库镜像 URL（国内直连 GitHub 不可达，mihomo 默认 URL 下载会 EOF 失败）
    let geo_base = format!("{MIRROR_PREFIX}{GEO_RELEASE_BASE}");
    let mut geox = Mapping::new();
    geox.insert(s("mmdb"), s(&format!("{geo_base}/geoip.metadb")));
    geox.insert(s("geoip"), s(&format!("{geo_base}/geoip.dat")));
    geox.insert(s("geosite"), s(&format!("{geo_base}/geosite.dat")));
    root.insert(s("geox-url"), Value::Mapping(geox));

    // TUN 模式：劫持全局流量到虚拟网卡（须 root 运行）。配 fake-ip DNS 与 dns-hijack。
    if params.tun {
        let mut tun = Mapping::new();
        tun.insert(s("enable"), Value::Bool(true));
        // stack=system：走 macOS 原生 utun + 内核 TCP 栈，不经用户态 goroutine。
        // gvisor 栈在连接风暴 + 批量超时失败时会泄漏 dial goroutine 进入 busy-loop
        // （唤醒后 iCloud/WPS 等数十 App 重连触发，CPU 卡 100% 不自愈）；system 栈
        // 连接管理交还内核，从根上消除该类泄漏，性能也更好。
        tun.insert(s("stack"), s("system"));
        tun.insert(s("dns-hijack"), Value::Sequence(vec![s("any:53")]));
        tun.insert(s("auto-route"), Value::Bool(true));
        tun.insert(s("auto-detect-interface"), Value::Bool(true));
        root.insert(s("tun"), Value::Mapping(tun));

        let mut dns = Mapping::new();
        dns.insert(s("enable"), Value::Bool(true));
        dns.insert(s("enhanced-mode"), s("fake-ip"));
        dns.insert(s("fake-ip-range"), s("198.18.0.1/16"));
        // nameserver 国内直连：fake-ip 查询 + DIRECT 流量真实解析（如 apple.com）均走此。
        // 国内 DNS 对常见域名（含未被污染的海外域名如 apple）返回正确 IP，快速可靠。
        // 不配 fallback/fallback-filter：海外 DoH 在 TUN 下经代理，会让 DIRECT 海外域名解析
        // 串行等待 fallback（实测 apple.com couldn't find ip），拖慢测速与直连（active 比 idle
        // 慢一个数量级的根因）。被污染域名（google 等）走代理远程解析，不依赖本地 DNS。
        dns.insert(
            s("nameserver"),
            Value::Sequence(vec![s("223.5.5.5"), s("119.29.29.29")]),
        );
        // proxy-server-nameserver：代理节点域名（server 字段，如 old.beibei1.top）专用解析，
        // 保证可靠解析 → mihomo 为节点 IP 添加 TUN 排除路由 → 避免 mihomo 到节点的流量被自身
        // TUN 接管形成回环（开启代理后全部 session 超时；idle 无 TUN 则正常）。
        dns.insert(
            s("proxy-server-nameserver"),
            Value::Sequence(vec![s("223.5.5.5"), s("119.29.29.29")]),
        );
        root.insert(s("dns"), Value::Mapping(dns));
    }

    if !all_proxies.is_empty() {
        root.insert(s("proxies"), Value::Sequence(all_proxies.clone()));
        let groups_val = match groups {
            Some(v) => override_group_urls(v),
            None => auto_groups(&all_proxies),
        };
        root.insert(s("proxy-groups"), groups_val);
        root.insert(s("rules"), rules.unwrap_or_else(default_rules));
    }

    serde_yml::to_string(&root).map_err(|e| format!("序列化 config.yaml 失败: {e}"))
}

/// 强制覆盖订阅自带 proxy-groups 中所有测速型分组（url-test / fallback / load-balance）
/// 的 `url` 字段为 HTTPS 框架统一 URL。订阅自带的 HTTP 测速 URL（如 gstatic.com）会被
/// 机场/ISP 劫持（mihomo 自警告 "hijacking test addresses"），致测速失败或数值偏高数倍。
/// interval/tolerance/lazy 等其他字段保留订阅原值（用户偏好）。
fn override_group_urls(groups: Value) -> Value {
    let arr = match groups {
        Value::Sequence(s) => s,
        v => return v,
    };
    let arr: Vec<Value> = arr
        .into_iter()
        .map(|g| {
            let mut m = match g {
                Value::Mapping(m) => m,
                v => return v,
            };
            let ty = m.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if matches!(ty, "url-test" | "fallback" | "load-balance") {
                m.insert(s("url"), s(DELAY_TEST_URL));
            }
            Value::Mapping(m)
        })
        .collect();
    Value::Sequence(arr)
}

/// 无订阅自带 groups 时自动生成：手动选择 + 自动测速。
fn auto_groups(proxies: &[Value]) -> Value {
    let names: Vec<Value> = proxies
        .iter()
        .filter_map(|p| p.get("name").cloned())
        .collect();
    let mut select_proxies = vec![s("DIRECT")];
    select_proxies.extend(names.iter().cloned());

    let mut select = Mapping::new();
    select.insert(s("name"), s("节点选择"));
    select.insert(s("type"), s("select"));
    select.insert(s("proxies"), Value::Sequence(select_proxies));

    let mut url_test = Mapping::new();
    url_test.insert(s("name"), s("自动选择"));
    url_test.insert(s("type"), s("url-test"));
    // 测速 URL 用 HTTPS Cloudflare generate_204（cp.cloudflare.com 国内 DNS 不污染，海外
    // anycast 快）；HTTP 会被机场/ISP 劫持（见 controller.rs DELAY_TEST_URL 注释）。
    url_test.insert(s("url"), s(DELAY_TEST_URL));
    url_test.insert(s("interval"), Value::from(300i64));
    url_test.insert(s("proxies"), Value::Sequence(names));

    Value::Sequence(vec![Value::Mapping(select), Value::Mapping(url_test)])
}

fn default_rules() -> Value {
    Value::Sequence(vec![s("GEOIP,CN,DIRECT"), s("MATCH,节点选择")])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> RunParams {
        RunParams {
            mixed_port: 7890,
            controller_port: 9090,
            secret: "s3cr3t".into(),
            mode: "rule".into(),
            active_sub_id: String::new(),
            tun: false,
        }
    }

    #[test]
    fn valid_sub_id_rejects_traversal() {
        assert!(valid_sub_id("a1b2c3"));
        assert!(!valid_sub_id(""));
        assert!(!valid_sub_id("../evil"));
        assert!(!valid_sub_id("a/b"));
        assert!(!valid_sub_id("a\\b"));
    }

    #[test]
    fn count_proxies_parses_yaml() {
        let yaml = "proxies:\n  - {name: A, type: ss}\n  - {name: B, type: vmess}\n";
        assert_eq!(count_proxies(yaml).unwrap(), 2);
        // 无 proxies 字段
        assert_eq!(count_proxies("mode: rule\n").unwrap(), 0);
        // 非法 YAML
        assert!(count_proxies("proxies: [unclosed").is_err());
    }

    #[test]
    fn merge_yaml_auto_groups_when_missing() {
        let yaml = "proxies:\n  - {name: HK-1, type: ss, server: a.com}\n  - {name: US-1, type: vmess, server: b.com}\n".to_string();
        let out = merge_yaml(&[yaml], &params()).unwrap();
        // 基础字段
        assert!(out.contains("mixed-port: 7890"));
        assert!(out.contains("127.0.0.1:9090"));
        // 自动生成 select / url-test 分组
        assert!(out.contains("节点选择"));
        assert!(out.contains("自动选择"));
        assert!(out.contains("HK-1"));
        assert!(out.contains("US-1"));
        // 默认规则
        assert!(out.contains("MATCH,节点选择"));
    }

    #[test]
    fn merge_yaml_reuses_subscription_groups() {
        let yaml = "proxies:\n  - {name: N1, type: ss}\nproxy-groups:\n  - {name: PROXY, type: select, proxies: [N1]}\nrules:\n  - MATCH,PROXY\n".to_string();
        let out = merge_yaml(&[yaml], &params()).unwrap();
        assert!(out.contains("PROXY"));
        assert!(out.contains("MATCH,PROXY"));
        // 不应注入自动分组（订阅自带了）
        assert!(!out.contains("节点选择"));
    }

    #[test]
    fn merge_yaml_overrides_subscription_group_urls() {
        // 订阅自带的 HTTP 测速 URL 会被机场劫持（mihomo 自警告），框架强制覆盖为 HTTPS。
        // url-test / fallback / load-balance 三类测速组都要覆盖；select 不带 url 不动。
        let yaml = "\
proxies:
  - {name: N1, type: ss}
proxy-groups:
  - {name: AUTO, type: url-test, url: 'http://www.gstatic.com/generate_204', interval: 60, proxies: [N1]}
  - {name: FB, type: fallback, url: 'http://www.gstatic.com/generate_204', interval: 60, proxies: [N1]}
  - {name: LB, type: load-balance, url: 'http://www.gstatic.com/generate_204', interval: 60, proxies: [N1]}
  - {name: SEL, type: select, proxies: [N1]}
".to_string();
        let out = merge_yaml(&[yaml], &params()).unwrap();
        // HTTP URL 必须被全部清除
        assert!(!out.contains("gstatic.com"));
        assert!(!out.contains("http://"));
        // 三类测速组都注入 HTTPS 框架 URL
        assert!(out.contains("url: https://cp.cloudflare.com/generate_204"));
        // 解析验证：每个测速组的 url 都是 HTTPS
        let v: Value = serde_yml::from_str(&out).unwrap();
        let groups = v.get("proxy-groups").and_then(|g| g.as_sequence()).unwrap();
        let test_groups: Vec<_> = groups
            .iter()
            .filter(|g| {
                matches!(
                    g.get("type").and_then(|t| t.as_str()),
                    Some("url-test") | Some("fallback") | Some("load-balance")
                )
            })
            .collect();
        assert_eq!(test_groups.len(), 3);
        for g in test_groups {
            assert_eq!(
                g.get("url").and_then(|u| u.as_str()),
                Some("https://cp.cloudflare.com/generate_204")
            );
        }
    }

    #[test]
    fn merge_yaml_dedup_by_name() {
        let a = "proxies:\n  - {name: DUP, type: ss}\n".to_string();
        let b = "proxies:\n  - {name: DUP, type: ss}\n  - {name: OK, type: ss}\n".to_string();
        let out = merge_yaml(&[a, b], &params()).unwrap();
        let v: Value = serde_yml::from_str(&out).unwrap();
        let count = v
            .get("proxies")
            .and_then(|p| p.as_sequence())
            .unwrap()
            .len();
        assert_eq!(count, 2); // DUP 去重，仅 DUP + OK
    }

    #[test]
    fn merge_yaml_no_proxies_minimal_config() {
        let out = merge_yaml(&[], &params()).unwrap();
        assert!(out.contains("mixed-port: 7890"));
        assert!(!out.contains("proxies:"));
        assert!(!out.contains("proxy-groups"));
    }

    #[test]
    fn merge_yaml_injects_global_perf_tunings() {
        // 全局性能开关恒注入（与 tun/订阅无关）—— unified-delay 缺失是测速数值偏高
        // 一个数量级的根因，tcp-concurrent/keep-alive-interval 降建连与复用开销
        let out = merge_yaml(&[], &params()).unwrap();
        assert!(out.contains("unified-delay: true"));
        assert!(out.contains("tcp-concurrent: true"));
        assert!(out.contains("keep-alive-interval: 30"));
    }

    #[test]
    fn merge_yaml_tun_section_when_enabled() {
        let mut p = params();
        p.tun = true;
        let out = merge_yaml(&[], &p).unwrap();
        assert!(out.contains("tun:"));
        assert!(out.contains("stack: system"));
        assert!(out.contains("auto-route: true"));
        assert!(out.contains("dns:"));
        assert!(out.contains("fake-ip"));
        // DNS：国内 nameserver 直连 + 节点域名专用 proxy-server-nameserver（防 TUN 回环）。
        // 不用 fallback（海外 DoH 经代理致 DIRECT 海外域名解析失败/等待，是 active 测速慢的根因）。
        assert!(out.contains("223.5.5.5"));
        assert!(out.contains("proxy-server-nameserver"));
        assert!(!out.contains("fallback"));
        // tun 关闭时不含 tun 段（idle 热重载）
        let out2 = merge_yaml(&[], &params()).unwrap();
        assert!(!out2.contains("tun:"));
    }

    #[test]
    fn merge_yaml_skips_invalid_yaml_keeps_valid() {
        let bad = "proxies: [unclosed".to_string();
        let good = "proxies:\n  - {name: OK, type: ss}\n".to_string();
        let out = merge_yaml(&[bad, good], &params()).unwrap();
        assert!(out.contains("OK"));
        let v: Value = serde_yml::from_str(&out).unwrap();
        let count = v
            .get("proxies")
            .and_then(|p| p.as_sequence())
            .map(|s| s.len())
            .unwrap_or(0);
        assert_eq!(count, 1);
    }

    #[test]
    fn merge_yaml_active_vs_idle_params() {
        let yaml = "proxies:\n  - {name: N, type: ss}\n".to_string();
        let mut active = params();
        active.tun = true;
        active.mode = "rule".into();
        let mut idle = params();
        idle.tun = false;
        idle.mode = "direct".into();
        let a = merge_yaml(&[yaml.clone()], &active).unwrap();
        let i = merge_yaml(&[yaml], &idle).unwrap();
        assert!(a.contains("tun:"));
        assert!(a.contains("mode: rule"));
        assert!(!i.contains("tun:"));
        assert!(i.contains("mode: direct"));
        // 两端都带节点，热重载只切 mode/tun
        assert!(a.contains("N") && i.contains("N"));
    }
}
