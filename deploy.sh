#!/usr/bin/env bash
# Voidnix 一键构建打包替换脚本
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "==> [1/6] lint: ESLint + UnoCSS 排序修复"
bun run lint

echo "==> [2/6] build: 前端类型检查 + Vite 构建"
bun run build

echo "==> [3/6] cargo check: Rust 编译检查"
(cd src-tauri && cargo check)

echo "==> [4/6] tauri build: 打包"
bun run tauri build

echo "==> [5/6] 替换 /Applications/Voidnix.app"
killall -9 Voidnix FinderExt 2>/dev/null || true
rm -rf /Applications/Voidnix.app
cp -R src-tauri/target/release/bundle/macos/Voidnix.app /Applications/

echo "==> [6/6] 嵌入 Finder 扩展"
scripts/embed.sh /Applications/Voidnix.app

echo ""
echo "完成。Voidnix 已更新至 /Applications/Voidnix.app"
