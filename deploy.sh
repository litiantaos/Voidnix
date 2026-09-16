#!/usr/bin/env bash
# Voidnix 一键打包部署脚本
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

if [ -f .env ]; then
  set -a; source .env; set +a
fi

echo "==> [1/5] tauri build: check + lint + 类型检查 + 前端构建 + Rust 编译 + 打包（zsh binary 经 bundle.resources 自动嵌入 Resources/）"
bun run tauri build

APP_SRC="src-tauri/target/release/bundle/macos/Voidnix.app"

# 关键防线：覆盖 /Applications 前先校验 target 产物签名。
# tauri build 凭 .env 的 APPLE_SIGNING_IDENTITY 用开发者证书 + hardened runtime 完整 deep
# 签名。若 .env 未加载或 keychain 锁定，build 会静默退化为 adhoc——
# adhoc 的 app 无稳定 DR 身份，TCC 只能退化为按 cdhash 匹配，每次部署（甚至每次重启）权限
# 都可能失效。codesign --verify 对 adhoc 同样返回成功，故须显式断言 TeamIdentifier（硬拦）。
# 证书类型仅警告不拦截：Apple Development 开发证书（未购 Apple Developer Program 时唯一可用）
# 同样带稳定 DR，可正常部署；其代价是 macOS 15+ 未公证应用的辅助功能/屏幕录制 API 授权
# 不可靠（prompt 条目无效，授权入口自动降级为系统设置手动添加，见 permission 分流）。
echo "==> [2/5] 校验产物签名（覆盖 /Applications 前拦截 adhoc 回归）"
codesign --verify --deep --strict "$APP_SRC"
SIG_DUMP="$(codesign -dvv "$APP_SRC" 2>&1)"
echo "$SIG_DUMP" | grep -E "Identifier|TeamIdentifier|Authority=Developer|flags"
echo "$SIG_DUMP" | grep -q "TeamIdentifier=NB22FJ9QD3" \
  || { echo "错误：签名退化为 adhoc（缺少 TeamIdentifier=NB22FJ9QD3），TCC 权限将失效，已中止"; exit 1; }
if echo "$SIG_DUMP" | grep -q "Authority=Developer ID Application"; then
  echo "签名证书：Developer ID Application ✓"
else
  echo "警告：签名证书非 Developer ID Application（当前为 Apple Development 开发证书，未公证）——"
  echo "  辅助功能/屏幕录制授权走系统设置手动添加（应用内已自动分流），屏幕录制可能周期性要求重授权。"
  echo "  一次到位需 Apple Developer Program：Developer ID Application 证书 + 公证（配齐下方凭证自动走）。"
fi

# 可选公证（macOS 15+ 辅助功能/屏幕录制 API 硬性要求）：.env 配齐 APPLE_ID /
# APPLE_PASSWORD（app 专用密码，appleid.apple.com 生成）/ APPLE_TEAM_ID 时，build 阶段
# tauri-bundler 已用同一组凭证自动完成 notarytool 公证 + staple；先 stapler validate，
# 已 staple 则跳过手动 submit（省数分钟与 Apple 公证配额），未 staple 才兜底补做。
# 未配齐则跳过并提示（权限授权可靠性见 AGENTS.md 签名一节）
if [ -n "$APPLE_ID" ] && [ -n "$APPLE_PASSWORD" ] && [ -n "$APPLE_TEAM_ID" ]; then
  if xcrun stapler validate "$APP_SRC" >/dev/null 2>&1; then
    echo "==> [3/5] 跳过公证（build 阶段 tauri-bundler 已完成公证 + staple）"
  else
    echo "==> [3/5] 公证（notarytool submit --wait）+ staple"
    NOTARY_ZIP="/tmp/Voidnix-notary.zip"
    ditto -c -k --keepParent "$APP_SRC" "$NOTARY_ZIP"
    xcrun notarytool submit "$NOTARY_ZIP" \
      --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" --team-id "$APPLE_TEAM_ID" --wait
    xcrun stapler staple "$APP_SRC"
    rm -f "$NOTARY_ZIP"
  fi
  spctl -a -vv "$APP_SRC"
else
  echo "==> [3/5] 跳过公证（.env 未配 APPLE_ID / APPLE_PASSWORD / APPLE_TEAM_ID）——辅助功能/屏幕录制授权不可靠"
fi

echo "==> [4/5] 替换 /Applications/Voidnix.app"
pkill -9 -f "Voidnix\.app" 2>/dev/null || true
rsync -a --delete "$APP_SRC/" /Applications/Voidnix.app/
codesign --verify --deep --strict /Applications/Voidnix.app \
  || { echo "错误：rsync 后签名校验失败，/Applications 可能损坏"; exit 1; }

# 清理 target 内的 .app 产物：已 rsync 到 /Applications，target 副本纯属多余。
# 不删则会被 Spotlight 索引，导致聚焦搜索出现两个 Voidnix（target 副本 + /Applications）。
# 仅删 .app，保留 .tar.gz/.sig（updater 产物）。
rm -rf "$APP_SRC"

echo "==> [5/5] 完成"
echo ""
echo "完成。Voidnix 已更新至 /Applications/Voidnix.app"
