#!/usr/bin/env bash
# 推送 main 与全部 tag 到 GitHub（触发 Actions 自动构建发布）
# 用法： bash publish.sh [代理地址]   默认代理 http://127.0.0.1:7890（Clash）
PROXY="${1:-http://127.0.0.1:7890}"
set -e
cd "$(dirname "$0")"
git -c http.proxy="$PROXY" -c https.proxy="$PROXY" push origin main
git -c http.proxy="$PROXY" -c https.proxy="$PROXY" push origin --tags
echo "已推送 main 与 tags。到 GitHub 仓库 Actions 页查看构建进度。"
