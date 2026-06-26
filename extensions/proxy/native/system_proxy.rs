use std::process::Command;

/// 解析 `networksetup -listallnetworkservices` 输出为活跃服务名列表。
/// 跳过标题行；排除空行与 `*` 前缀（禁用服务）。
pub(crate) fn parse_active_services(output: &str) -> Vec<String> {
    output
        .lines()
        .skip(1)
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('*'))
        .map(String::from)
        .collect()
}

/// 枚举活跃网络服务。
fn list_active_services() -> Result<Vec<String>, String> {
    let out = Command::new("networksetup")
        .arg("-listallnetworkservices")
        .output()
        .map_err(|e| format!("networksetup 调用失败: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(parse_active_services(&String::from_utf8_lossy(&out.stdout)))
}

/// 对单个服务执行 3 类代理（web/secureweb/socksfirewall）的设/清。
/// 单服务失败不阻断（虚拟服务等可能不支持），仅记日志。
fn run_for_service(svc: &str, port: u16, enable: bool) {
    let port_s = port.to_string();
    let cmds: Vec<Vec<String>> = if enable {
        vec![
            vec![
                "-setwebproxy".into(),
                svc.into(),
                "127.0.0.1".into(),
                port_s.clone(),
            ],
            vec![
                "-setsecurewebproxy".into(),
                svc.into(),
                "127.0.0.1".into(),
                port_s.clone(),
            ],
            vec![
                "-setsocksfirewallproxy".into(),
                svc.into(),
                "127.0.0.1".into(),
                port_s,
            ],
        ]
    } else {
        vec![
            vec!["-setwebproxystate".into(), svc.into(), "off".into()],
            vec!["-setsecurewebproxystate".into(), svc.into(), "off".into()],
            vec![
                "-setsocksfirewallproxystate".into(),
                svc.into(),
                "off".into(),
            ],
        ]
    };
    for args in cmds {
        let res = Command::new("networksetup").args(&args).output();
        if let Ok(out) = res {
            if !out.status.success() {
                eprintln!(
                    "[proxy] networksetup {:?}: {}",
                    args,
                    String::from_utf8_lossy(&out.stderr).trim()
                );
            }
        }
    }
}

/// 设/清系统代理（所有活跃网络服务的 HTTP/HTTPS/SOCKS 指向 127.0.0.1:port）。
///
/// macOS GUI 会话下 networksetup 无需 root（经 osascript 提权探测验证）。
/// 清除（enable=false）时 port 不使用。
pub fn apply(port: u16, enable: bool) -> Result<(), String> {
    let services = list_active_services()?;
    if services.is_empty() {
        return Err("未检测到活跃网络服务".to_string());
    }
    for svc in &services {
        run_for_service(svc, port, enable);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_skips_header_and_disabled() {
        let out = "An asterisk (*) denotes that a network service is disabled.\n\
                   Wi-Fi\n\
                   *VPN Disabled\n\
                   Thunderbolt Bridge\n";
        assert_eq!(
            parse_active_services(out),
            vec!["Wi-Fi", "Thunderbolt Bridge"]
        );
    }

    #[test]
    fn parse_empty_output() {
        assert_eq!(
            parse_active_services("An asterisk (*) denotes...\n"),
            Vec::<String>::new()
        );
    }
}
