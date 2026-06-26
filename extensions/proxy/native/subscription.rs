use crate::http;
use crate::runtime::storage::ext_data_dir;
use serde_yaml::{Mapping, Value};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;
use tauri::AppHandle;

use super::core::RunParams;

/// 订阅请求 UA：部分机场据此返回 Clash YAML 而非 Base64 订阅，与 mihomo 版本对齐。
const SUB_UA: &str = "clash.meta/v1.19.27";

fn sub_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = ext_data_dir(app, "proxy")?.join("subs");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn sub_yaml_path(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
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
    let val: Value = serde_yaml::from_str(yaml_text).map_err(|e| format!("非 Clash YAML: {e}"))?;
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

/// 读取 subs/*.yaml 收集原始文本，交 merge_yaml 合并生成 config.yaml 文本。
pub fn build_run_config(app: &AppHandle, params: &RunParams) -> Result<String, String> {
    let dir = ext_data_dir(app, "proxy")?.join("subs");
    let mut texts: Vec<String> = Vec::new();
    if dir.exists() {
        for entry in std::fs::read_dir(&dir)
            .map_err(|e| e.to_string())?
            .flatten()
        {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
                continue;
            }
            if let Ok(t) = std::fs::read_to_string(&path) {
                texts.push(t);
            }
        }
    }
    merge_yaml(&texts, params)
}

/// 合并多份 Clash YAML 文本 + 基础配置 → config.yaml 文本（纯函数，便于单测）。
///
/// 多订阅合并策略（MVP）：
/// - proxies：所有订阅按 name 去重拼接
/// - proxy-groups：取首个含非空 proxy-groups 的订阅；否则自动生成 select + url-test
/// - rules：取首个含非空 rules 的订阅；否则默认 GEOIP CN 直连 + MATCH 代理
pub fn merge_yaml(texts: &[String], params: &RunParams) -> Result<String, String> {
    let mut all_proxies: Vec<Value> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut groups: Option<Value> = None;
    let mut rules: Option<Value> = None;

    for text in texts {
        let val: Value = match serde_yaml::from_str(text) {
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
    root.insert(s("log-level"), s("warning"));
    root.insert(s("allow-lan"), Value::Bool(false));

    // TUN 模式：劫持全局流量到虚拟网卡（须 root 运行）。配 fake-ip DNS 与 dns-hijack。
    if params.tun {
        let mut tun = Mapping::new();
        tun.insert(s("enable"), Value::Bool(true));
        tun.insert(s("stack"), s("gvisor"));
        tun.insert(s("dns-hijack"), Value::Sequence(vec![s("any:53")]));
        tun.insert(s("auto-route"), Value::Bool(true));
        tun.insert(s("auto-detect-interface"), Value::Bool(true));
        root.insert(s("tun"), Value::Mapping(tun));

        let mut dns = Mapping::new();
        dns.insert(s("enable"), Value::Bool(true));
        dns.insert(s("enhanced-mode"), s("fake-ip"));
        dns.insert(
            s("nameserver"),
            Value::Sequence(vec![
                s("https://dns.google/dns-query"),
                s("https://1.1.1.1/dns-query"),
                s("223.5.5.5"),
            ]),
        );
        root.insert(s("dns"), Value::Mapping(dns));
    }

    if !all_proxies.is_empty() {
        root.insert(s("proxies"), Value::Sequence(all_proxies.clone()));
        let groups_val = groups.unwrap_or_else(|| auto_groups(&all_proxies));
        root.insert(s("proxy-groups"), groups_val);
        root.insert(s("rules"), rules.unwrap_or_else(default_rules));
    }

    serde_yaml::to_string(&root).map_err(|e| format!("序列化 config.yaml 失败: {e}"))
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
    select.insert(s("name"), s("🚀 节点选择"));
    select.insert(s("type"), s("select"));
    select.insert(s("proxies"), Value::Sequence(select_proxies));

    let mut url_test = Mapping::new();
    url_test.insert(s("name"), s("♻️ 自动选择"));
    url_test.insert(s("type"), s("url-test"));
    url_test.insert(s("url"), s("http://www.gstatic.com/generate_204"));
    url_test.insert(s("interval"), Value::from(300i64));
    url_test.insert(s("proxies"), Value::Sequence(names));

    Value::Sequence(vec![Value::Mapping(select), Value::Mapping(url_test)])
}

fn default_rules() -> Value {
    Value::Sequence(vec![s("GEOIP,CN,DIRECT"), s("MATCH,🚀 节点选择")])
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
            tun: false,
        }
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
        assert!(out.contains("🚀 节点选择"));
        assert!(out.contains("♻️ 自动选择"));
        assert!(out.contains("HK-1"));
        assert!(out.contains("US-1"));
        // 默认规则
        assert!(out.contains("MATCH,🚀 节点选择"));
    }

    #[test]
    fn merge_yaml_reuses_subscription_groups() {
        let yaml = "proxies:\n  - {name: N1, type: ss}\nproxy-groups:\n  - {name: PROXY, type: select, proxies: [N1]}\nrules:\n  - MATCH,PROXY\n".to_string();
        let out = merge_yaml(&[yaml], &params()).unwrap();
        assert!(out.contains("PROXY"));
        assert!(out.contains("MATCH,PROXY"));
        // 不应注入自动分组（订阅自带了）
        assert!(!out.contains("🚀 节点选择"));
    }

    #[test]
    fn merge_yaml_dedup_by_name() {
        let a = "proxies:\n  - {name: DUP, type: ss}\n".to_string();
        let b = "proxies:\n  - {name: DUP, type: ss}\n  - {name: OK, type: ss}\n".to_string();
        let out = merge_yaml(&[a, b], &params()).unwrap();
        let v: Value = serde_yaml::from_str(&out).unwrap();
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
    fn merge_yaml_tun_section_when_enabled() {
        let mut p = params();
        p.tun = true;
        let out = merge_yaml(&[], &p).unwrap();
        assert!(out.contains("tun:"));
        assert!(out.contains("stack: gvisor"));
        assert!(out.contains("auto-route: true"));
        assert!(out.contains("dns:"));
        assert!(out.contains("fake-ip"));
        // tun 关闭时不含 tun 段
        let out2 = merge_yaml(&[], &params()).unwrap();
        assert!(!out2.contains("tun:"));
    }
}
