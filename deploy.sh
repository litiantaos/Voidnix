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

APP_SRC="src-tauri/target/release/bundle/macos/Voidnix.app"

# 关键防线：覆盖 /Applications 前先校验 target 产物签名。
# tauri build 凭 .env 的 APPLE_SIGNING_IDENTITY 用开发者证书 + hardened runtime 完整 deep
# 签名。若 .env 未加载或 keychain 锁定，build 会静默退化为 adhoc——
# adhoc 的 app 无稳定 DR 身份，TCC 只能退化为按 cdhash 匹配，每次部署（甚至每次重启）权限
# 都可能失效。codesign --verify 对 adhoc 同样返回成功，故须显式断言 TeamIdentifier。
echo "==> [2/3] 校验产物签名（覆盖 /Applications 前拦截 adhoc 回归）"
codesign --verify --deep --strict "$APP_SRC"
SIG_DUMP="$(codesign -dvv "$APP_SRC" 2>&1)"
echo "$SIG_DUMP" | grep -E "Identifier|TeamIdentifier|flags"
echo "$SIG_DUMP" | grep -q "TeamIdentifier=27869WH3RZ" \
  || { echo "错误：签名退化为 adhoc（缺少 TeamIdentifier=27869WH3RZ），TCC 权限将失效，已中止"; exit 1; }

echo "==> [3/3] 替换 /Applications/Voidnix.app"
pkill -9 -f "Voidnix\.app" 2>/dev/null || true
rsync -a --delete "$APP_SRC/" /Applications/Voidnix.app/
codesign --verify --deep --strict /Applications/Voidnix.app \
  || { echo "错误：rsync 后签名校验失败，/Applications 可能损坏"; exit 1; }

# 清理 target 内的 .app 产物：已 rsync 到 /Applications，target 副本纯属多余。
# 不删则会被 Spotlight 索引，导致聚焦搜索出现两个 Voidnix（target 副本 + /Applications）。
# 仅删 .app，保留 .tar.gz/.sig（updater 产物）。
rm -rf "$APP_SRC"

echo ""
echo "完成。Voidnix 已更新至 /Applications/Voidnix.app"
