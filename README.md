# Voidnix

macOS 效率启动器。模块化架构，极简界面，原生性能。

## 技术栈

Tauri + Rust | Vue 3 + Vite + Bun | UnoCSS (preset-wind4) | Pinia | SQLite

## 功能

- [x] 应用与文件搜索（Spotlight 召回 + Nucleo 精排，拼音/中英混搜）
- [x] 剪贴板历史（天数清理、收藏、智能过滤）
- [x] 选词翻译
- [x] AI Chat
- [x] 系统唤醒
- [x] 计算器（输入数学表达式触发）
- [x] 汇率转换 / UUID / 时间戳 / IP / Base64（斜杠命令触发）
- [x] 右键菜单
- [x] 截屏标注 + OCR
- [ ] 录屏 + 轻剪
- [ ] 视频处理
- [ ] 窗口管理（快捷操作窗口尺寸/位置/布局）
- [ ] 终端辅助（在终端中输入时显示一个小窗自动联想并补全）

## 快捷键

- `Cmd+Shift+Space`：唤起/隐藏主窗口
- `Cmd+Shift+C`：唤起剪贴板历史
- `Tab`：切换选项卡
- `↑↓`：切换列表选项
- `Enter`：执行选中项
- `Escape`：清空搜索 / 退回主界面

## 开发

```bash
bun install                  # 安装依赖
bun run tauri dev            # 开发模式
bun run build                # 前端构建检查
bun run lint                 # 格式化 + UnoCSS 排序
bun run gen:bindings         # 重新生成 src/bindings.ts（修改 Rust 结构体后执行）
cd src-tauri && cargo check  # Rust 编译检查
```

## 权限

- **辅助功能**：全局快捷键、剪贴板粘贴
- **屏幕录制**：截屏/录屏
- **无需权限**：OCR（Apple Vision）
