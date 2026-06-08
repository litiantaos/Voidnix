use super::ext_manifest::{load_from_dir, tier2_extensions_dir, LoadedExtension, Manifest};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tauri::AppHandle;

/// Tier 2 扩展加载器
pub struct ExtensionLoader {
    /// 已加载的扩展（id -> 扩展元数据）
    extensions: RwLock<HashMap<String, Arc<LoadedExtension>>>,
}

fn dev_extensions_dir() -> Option<PathBuf> {
    let env_dir = std::env::var("VOIDNIX_DEV_EXTENSIONS").ok().map(PathBuf::from);
    if env_dir.is_some() {
        return env_dir;
    }

    #[cfg(debug_assertions)]
    {
        std::env::current_exe().ok().and_then(|exe| {
            let mut dir = exe.parent()?;
            for _ in 0..6 {
                let has_cargo = dir.join("Cargo.toml").exists()
                    || dir.join("src-tauri/Cargo.toml").exists();
                if has_cargo && dir.join("extensions").is_dir() {
                    return Some(dir.join("extensions"));
                }
                dir = dir.parent()?;
            }
            None
        })
    }

    #[cfg(not(debug_assertions))]
    {
        None
    }
}

impl ExtensionLoader {
    pub fn new() -> Self {
        Self {
            extensions: RwLock::new(HashMap::new()),
        }
    }

    /// 扫描 Tier 2 扩展目录，加载所有已安装扩展
    pub fn rescan(&self, app: &AppHandle) -> Result<Vec<String>, String> {
        let mut loaded_ids = Vec::new();
        let mut extensions = self.extensions.write().unwrap();

        let mut scan_dir = |dir: &Path, ids: &mut Vec<String>| {
            if !dir.exists() {
                return;
            }
            let readdir = match std::fs::read_dir(dir) {
                Ok(r) => r,
                Err(_) => return,
            };
            for entry in readdir.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                if let Ok(loaded) = load_from_dir(&path) {
                    let id = loaded.manifest.extension.id.clone();
                    extensions.insert(id.clone(), Arc::new(loaded));
                    ids.push(id);
                }
            }
        };

        scan_dir(&tier2_extensions_dir(app), &mut loaded_ids);

        if let Some(dev_dir) = dev_extensions_dir() {
            scan_dir(&dev_dir, &mut loaded_ids);
        }

        Ok(loaded_ids)
    }

    /// 安装 .vnext 包（zip 文件）
    pub fn install(&self, app: &AppHandle, vnext_path: &Path) -> Result<String, String> {
        use std::io::Read;

        let file = std::fs::File::open(vnext_path)
            .map_err(|e| format!("Failed to open .vnext: {}", e))?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| format!("Failed to parse zip: {}", e))?;

        // 提取 manifest.toml 获取扩展 id
        let manifest_bytes = {
            let mut file = archive
                .by_name("manifest.toml")
                .map_err(|e| format!("Missing manifest.toml: {}", e))?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)
                .map_err(|e| format!("Failed to read manifest: {}", e))?;
            bytes
        };
        let manifest: Manifest = toml::from_str(&String::from_utf8_lossy(&manifest_bytes))
            .map_err(|e| format!("Failed to parse manifest: {}", e))?;
        let id = manifest.extension.id.clone();

        // 目标目录
        let ext_dir = tier2_extensions_dir(app).join(&id);
        let _ = std::fs::create_dir_all(&ext_dir);

        // 解压所有文件
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(|e| format!("get file: {}", e))?;
            let name = file.name().to_string();
            if name.ends_with('/') {
                let _ = std::fs::create_dir_all(ext_dir.join(&name));
            } else {
                let out_path = ext_dir.join(&name);
                if let Some(parent) = out_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let mut out_file = std::fs::File::create(&out_path)
                    .map_err(|e| format!("create {:?}: {}", out_path, e))?;
                std::io::copy(&mut file, &mut out_file)
                    .map_err(|e| format!("write {:?}: {}", out_path, e))?;
            }
        }

        // 加载到内存
        let loaded = load_from_dir(&ext_dir)?;
        {
            let mut extensions = self.extensions.write().unwrap();
            extensions.insert(id.clone(), Arc::new(loaded));
        }

        Ok(id)
    }

    /// 卸载扩展
    pub fn uninstall(&self, app: &AppHandle, id: &str) -> Result<(), String> {
        {
            let mut extensions = self.extensions.write().unwrap();
            extensions.remove(id);
        }

        let ext_dir = tier2_extensions_dir(app).join(id);
        if ext_dir.exists() {
            std::fs::remove_dir_all(&ext_dir)
                .map_err(|e| format!("Failed to remove directory: {}", e))?;
        }

        Ok(())
    }

    /// 获取扩展元数据
    pub fn get(&self, id: &str) -> Option<Arc<LoadedExtension>> {
        self.extensions.read().unwrap().get(id).cloned()
    }

    /// 列出所有已加载扩展
    pub fn list(&self) -> Vec<Arc<LoadedExtension>> {
        self.extensions.read().unwrap().values().cloned().collect()
    }

    /// 获取扩展入口文件的绝对路径
    pub fn entry_path(&self, id: &str) -> Option<PathBuf> {
        let ext = self.get(id)?;
        let manifest = &ext.manifest;
        Some(ext.source_dir.join(&manifest.entry.main))
    }

    /// 获取扩展入口文件内容
    pub fn entry_content(&self, id: &str) -> Option<String> {
        let path = self.entry_path(id)?;
        std::fs::read_to_string(&path).ok()
    }

    /// 获取扩展 README 内容
    pub fn readme_content(&self, id: &str) -> Option<String> {
        let ext = self.get(id)?;
        let readme = ext.source_dir.join("README.md");
        if readme.exists() {
            std::fs::read_to_string(&readme).ok()
        } else {
            None
        }
    }
}

impl Default for ExtensionLoader {
    fn default() -> Self {
        Self::new()
    }
}
