// 阻止在 Windows 发布版中显示额外的控制台窗口，请勿移除！！
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    app_lib::run()
}
