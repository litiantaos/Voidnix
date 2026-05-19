use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct IpInfo {
    pub ip: Option<String>,
    pub success: Option<bool>,
    pub message: Option<String>,
    pub country: Option<String>,
    pub region: Option<String>,
    pub city: Option<String>,
    pub isp: Option<String>,
    pub org: Option<String>,
    pub asn: Option<String>,
}

#[tauri::command]
#[cfg_attr(feature = "specta", specta::specta)]
pub async fn fetch_ip_info(ip: Option<String>) -> Result<IpInfo, String> {
    let url = match ip {
        Some(ip_addr) if !ip_addr.trim().is_empty() => format!("https://ipwhois.app/json/{}?lang=zh-CN", ip_addr.trim()),
        _ => "https://ipwhois.app/json/?lang=zh-CN".to_string(),
    };

    let response = crate::infra::http::client()
        .get(&url)
        .header("User-Agent", "Launcher/1.0")
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if response.status().is_success() {
        let info = response
            .json::<IpInfo>()
            .await
            .map_err(|e| format!("JSON parsing error: {}", e))?;
        Ok(info)
    } else {
        Err(format!("HTTP Error: {}", response.status()))
    }
}


pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new("ip").build()
}
