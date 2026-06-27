// 统一菜单栏：框架唯一托盘图标，聚合所有扩展贡献的菜单项。
//
// 镜像 `shortcut.rs` 的扩展钩子注册范式（LazyLock<Mutex<Vec>> + free function）。
// 扩展在 setup 内 `register` 一个贡献段，状态变化后 `refresh` 触发重建。
// 图标可见性 = Σ 各段 build() 项数 > 0（无需 active flag，扩展开/关状态天然反映在 build 返回空/非空）。
// 托盘用 Tauri 跨平台 tray API（非裸 NSStatusItem），故归 runtime（平台无关）而非 platform。

use std::sync::{Arc, LazyLock, Mutex};

use tauri::menu::{
    CheckMenuItem, IsMenuItem, Menu, MenuBuilder, MenuEvent, MenuItem, PredefinedMenuItem,
    SubmenuBuilder,
};
use tauri::tray::TrayIconBuilder;
use tauri::AppHandle;

use crate::runtime::lock_or_recover;

/// 框架唯一的菜单栏托盘 id。
const TRAY_ID: &str = "voidnix_menubar";

/// 扩展供给的菜单项描述（框架不定义业务语义）。
#[derive(Clone)]
pub enum MenuEntry {
    /// 普通文本项。
    Item {
        id: String,
        label: String,
        enabled: bool,
    },
    /// 带勾选标记的项（节点选中 / 开关状态）。
    CheckItem {
        id: String,
        label: String,
        checked: bool,
    },
    /// 子菜单（如「切换节点」）。
    Submenu {
        label: String,
        items: Vec<MenuEntry>,
    },
    /// 分隔线。
    Separator,
}

/// 扩展向聚合菜单贡献的一个段落。
///
/// - `title`：分组标题（disabled 项渲染，如「保持唤醒」/「代理」）。
/// - `build`：返回当前快照；空 Vec = 该扩展当前不贡献（不参与菜单、不影响图标可见性）。
/// - `on_event`：收到所有点击的 item id，扩展自行过滤归属项（约定 id 以扩展 id 为前缀避免碰撞）。
///
/// `build`/`on_event` 为 `Arc<dyn Fn>` 以便在锁外调用：on_event 内部常触发状态变更 → `refresh`，
/// 若持锁调用将重入死锁（std::sync::Mutex 非重入）。
pub struct MenuBarContribution {
    pub title: &'static str,
    pub build: MenuBuild,
    pub on_event: MenuOnEvent,
}

type MenuBuild = Arc<dyn Fn(&AppHandle) -> Vec<MenuEntry> + Send + Sync>;
type MenuOnEvent = Arc<dyn Fn(&AppHandle, &str) + Send + Sync>;

static CONTRIBUTIONS: LazyLock<Mutex<Vec<MenuBarContribution>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

/// 注册一个菜单栏贡献段（扩展 setup 内调用）。
pub fn register(contribution: MenuBarContribution) {
    lock_or_recover(&CONTRIBUTIONS).push(contribution);
}

/// 重建聚合菜单 + 按总项数显隐图标（扩展状态变化后调用）。
pub fn refresh(app: &AppHandle) {
    rebuild(app);
}

fn rebuild(app: &AppHandle) {
    // 锁内仅克隆 title + build 句柄，锁外调用 build 闭包（防 build/on_event → refresh 重入死锁）
    let specs: Vec<(&'static str, MenuBuild)> = {
        let guard = lock_or_recover(&CONTRIBUTIONS);
        guard.iter().map(|c| (c.title, c.build.clone())).collect()
    };
    // build 快照，过滤空段
    let sections: Vec<(&'static str, Vec<MenuEntry>)> = specs
        .iter()
        .map(|(title, build)| (*title, build(app)))
        .filter(|(_, items)| !items.is_empty())
        .collect();

    if sections.is_empty() {
        // 无扩展贡献：隐藏图标（托盘未创建则无需操作）
        if let Some(tray) = app.tray_by_id(TRAY_ID) {
            let _ = tray.set_visible(false);
        }
        return;
    }

    if let Err(e) = ensure_tray(app) {
        eprintln!("[menubar] ensure tray: {e}");
        return;
    }
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };

    // 按扩展名称分组：每段前插 disabled 标题项，段间加分隔线
    let mut entries: Vec<MenuEntry> = Vec::new();
    for (i, (title, items)) in sections.iter().enumerate() {
        if i > 0 {
            entries.push(MenuEntry::Separator);
        }
        entries.push(MenuEntry::Item {
            id: format!("__section_{i}"),
            label: title.to_string(),
            enabled: false,
        });
        entries.extend(items.iter().cloned());
    }

    match build_menu(app, &entries) {
        Ok(menu) => {
            let _ = tray.set_menu(Some(menu));
            let _ = tray.set_visible(true);
        }
        Err(e) => eprintln!("[menubar] build menu: {e}"),
    }
}

/// 构建托盘（若已存在则跳过）。惰性创建，复用不销毁。
fn ensure_tray(app: &AppHandle) -> Result<(), String> {
    if app.tray_by_id(TRAY_ID).is_some() {
        return Ok(());
    }
    let icon = tauri::image::Image::from_bytes(include_bytes!("../../../public/bar_icon.png"))
        .map_err(|e| e.to_string())?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .icon_as_template(true)
        .tooltip("Voidnix")
        .show_menu_on_left_click(true)
        .on_menu_event(dispatch_event)
        .build(app)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 菜单点击分派：锁内克隆 on_event 句柄，锁外逐个调用（防重入死锁）。
fn dispatch_event(app: &AppHandle, event: MenuEvent) {
    let id = event.id().as_ref();
    let handlers: Vec<MenuOnEvent> = lock_or_recover(&CONTRIBUTIONS)
        .iter()
        .map(|c| c.on_event.clone())
        .collect();
    for h in handlers {
        h(app, id);
    }
}

/// 递归把 MenuEntry 列表构建为原生 Menu。
fn build_menu(app: &AppHandle, entries: &[MenuEntry]) -> Result<Menu<tauri::Wry>, String> {
    let owned = entries_to_items(app, entries)?;
    let refs: Vec<&dyn IsMenuItem<tauri::Wry>> = owned.iter().map(|b| &**b).collect();
    MenuBuilder::new(app)
        .items(&refs)
        .build()
        .map_err(|e| e.to_string())
}

/// MenuEntry → 拥有的 IsMenuItem（子菜单的子项存活至本次 build 完成，NSMenu 底层已 retain）。
fn entries_to_items(
    app: &AppHandle,
    entries: &[MenuEntry],
) -> Result<Vec<Box<dyn IsMenuItem<tauri::Wry>>>, String> {
    let mut out: Vec<Box<dyn IsMenuItem<tauri::Wry>>> = Vec::new();
    for e in entries {
        match e {
            MenuEntry::Item { id, label, enabled } => {
                let it = MenuItem::with_id(app, id.clone(), label.as_str(), *enabled, None::<&str>)
                    .map_err(|e| e.to_string())?;
                out.push(Box::new(it));
            }
            MenuEntry::CheckItem { id, label, checked } => {
                let it = CheckMenuItem::with_id(
                    app,
                    id.clone(),
                    label.as_str(),
                    true,
                    *checked,
                    None::<&str>,
                )
                .map_err(|e| e.to_string())?;
                out.push(Box::new(it));
            }
            MenuEntry::Submenu { label, items } => {
                let child = entries_to_items(app, items)?;
                let refs: Vec<&dyn IsMenuItem<tauri::Wry>> = child.iter().map(|b| &**b).collect();
                let sub = SubmenuBuilder::new(app, label.as_str())
                    .items(&refs)
                    .build()
                    .map_err(|e| e.to_string())?;
                out.push(Box::new(sub));
            }
            MenuEntry::Separator => {
                let it = PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?;
                out.push(Box::new(it));
            }
        }
    }
    Ok(out)
}
