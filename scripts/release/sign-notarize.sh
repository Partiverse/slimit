#!/usr/bin/env bash
# Slimit macOS 签名 + 公证（docs/W8-PLAN.md §1 的可执行版）。
# 前置：Apple Developer Program 证书（Developer ID Application）。
# 全部凭据只走环境变量，绝不写入本文件或仓库；示例值见 W8-PLAN.md。
#
# 用法（四个变量必须先导出，含义见 W8-PLAN.md §1）：
#   APPLE_SIGNING_IDENTITY / APPLE_ID / APPLE_PASSWORD / APPLE_TEAM_ID
#   bash scripts/release/sign-notarize.sh    # 先 cargo tauri build
set -euo pipefail

APP="target/release/bundle/macos/Slimit.app"
DMG="target/release/bundle/dmg/Slimit_0.1.0_aarch64.dmg"

echo "== 0/4 环境变量自检 =="
: "${APPLE_SIGNING_IDENTITY:?缺少 APPLE_SIGNING_IDENTITY（security find-identity -v -p codesigning 查看）}"
: "${APPLE_ID:?缺少 APPLE_ID}"
: "${APPLE_PASSWORD:?缺少 APPLE_PASSWORD（appleid.apple.com 生成的 App 专用密码）}"
: "${APPLE_TEAM_ID:?缺少 APPLE_TEAM_ID}"

echo "== 1/4 签名 .app（runtime hardened）=="
codesign --force --deep --options runtime --sign "$APPLE_SIGNING_IDENTITY" "$APP"
codesign --verify --verbose=2 "$APP"

echo "== 2/4 公证 DMG（notarytool，等待结果）=="
xcrun notarytool submit "$DMG" \
  --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" --team-id "$APPLE_TEAM_ID" --wait

echo "== 3/4 staple（把公证票据钉到产物）=="
xcrun stapler staple "$DMG"
xcrun stapler staple "$APP"

echo "== 4/4 验收 =="
spctl -a -vv "$APP"
xcrun stapler validate "$DMG" && xcrun stapler validate "$APP"
echo "完成：产物已签名+公证，可公开发布。"
