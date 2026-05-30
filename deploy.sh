#!/usr/bin/env bash
# Voidnix 一键打包部署脚本
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "==> [1/3] tauri build: 格式化 + lint + 类型检查 + 前端构建 + Rust 编译 + 打包"
bun run tauri build

echo "==> [2/3] 替换 /Applications/Voidnix.app"
pkill -9 -f "Voidnix\.app" 2>/dev/null || true
killall -9 FinderExt 2>/dev/null || true
rsync -a --delete src-tauri/target/release/bundle/macos/Voidnix.app/ /Applications/Voidnix.app/

echo "==> [3/3] 嵌入 Finder 扩展"
scripts/embed.sh /Applications/Voidnix.app

echo ""
echo "完成。Voidnix 已更新至 /Applications/Voidnix.app"
