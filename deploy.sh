#!/usr/bin/env bash
# Voidnix 一键打包部署脚本
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

if [ -f .env ]; then
  set -a; source .env; set +a
fi

echo "==> [1/3] tauri build: check + lint + 类型检查 + 前端构建 + Rust 编译 + 打包（zsh binary 经 bundle.resources 自动嵌入 Resources/）"
bun run tauri build

echo "==> [2/3] 替换 /Applications/Voidnix.app"
pkill -9 -f "Voidnix\.app" 2>/dev/null || true
rsync -a --delete src-tauri/target/release/bundle/macos/Voidnix.app/ /Applications/Voidnix.app/

echo "==> [3/3] 开发者证书签名"
# tauri build 默认 adhoc 签名，TCC csreq 校验不通过会导致 AX/屏幕录制等权限静默失效。
# 用 Apple Development 证书重签名使 csreq 匹配 TCC 记录，权限无需每次重新授权。
SIGN_IDENTITY="${APPLE_SIGNING_IDENTITY:-Apple Development: admin@litiantao.com (6VJZLBK8LU)}"
codesign --force --sign "$SIGN_IDENTITY" /Applications/Voidnix.app

echo ""
echo "完成。Voidnix 已更新至 /Applications/Voidnix.app"

echo "==> 签名验证"
codesign -dvv /Applications/Voidnix.app 2>&1 | grep -E "Identifier|TeamIdentifier|Signature"
