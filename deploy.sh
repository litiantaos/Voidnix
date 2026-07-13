#!/usr/bin/env bash
# Voidnix 一键打包部署脚本
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

if [ -f .env ]; then
  set -a; source .env; set +a
fi

echo "==> [1/2] tauri build: check + lint + 类型检查 + 前端构建 + Rust 编译 + 打包（zsh binary 经 bundle.resources 自动嵌入 Resources/）"
bun run tauri build

echo "==> [2/2] 替换 /Applications/Voidnix.app"
pkill -9 -f "Voidnix\.app" 2>/dev/null || true
rsync -a --delete src-tauri/target/release/bundle/macos/Voidnix.app/ /Applications/Voidnix.app/

echo ""
echo "完成。Voidnix 已更新至 /Applications/Voidnix.app"
