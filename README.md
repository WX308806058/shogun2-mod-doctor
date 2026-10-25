# 幕府将军2 Mod 冲突检测器 (Shogun2 Mod Doctor)

技术栈：**Tauri 2 + Vue 3 + Vite 4 + TypeScript + Rust**

检测《全面战争：幕府将军2》已启用 MOD 的冲突情况，重点排查**加载页闪退**类问题。

## 功能

1. **包结构体检** — PFH 头 / 索引 / 数据越界 / 0 字节文件
2. **同名表覆盖链** — 按游戏真实加载顺序（boot → release → patch → mod(used_mods.txt 顺序) → movie）标出每个 db 表的最终"赢家"，版本/布局不一致告警
3. **科技生态完整性**（加载页闪退高发根因）
   - 科技表 14 字段严格解析、重复 key、缺失原生 key
   - 5 张科技引用表（technology_nodes / effects_junction / ai_path / building_required / unit_required）悬空 key 检查
   - mod 包空表(0 行)覆盖原生表 → 高危警告
4. **CLI 模式** — 无界面输出 JSON

## 开发

```bash
npm install
npm run tauri dev    # 开发窗口
```

## 发布（GitHub Actions 自动构建）

本地推送 tag 即可，云端 windows-latest 自动构建并发布 Release：

```bash
# 需要 Clash 代理开启（按实际端口调整）
git -c http.proxy=http://127.0.0.1:7890 push origin main --tags
```

或使用辅助脚本（内部调用上面同款命令）：

```bash
bash publish.sh          # 推送 main + 全部 tag
```

## 扫描进度弹窗

点击「开始扫描」后显示进度弹窗，实时显示当前正在解析的 pack 名称与进度
（后端通过 Tauri 事件 `scan-progress` 推送 `{current, total, name}`）。

## CLI 用法

```bash
src-tauri/target/release/shogun2-mod-doctor.exe scan "D:/Program Files/Steam/steamapps/common/Total War SHOGUN 2" [--all]
# --all: 扫描 data 目录全部 mod（含未启用）
```

## 关键实现说明

- pack 解析与 `mod_doctor.py` 逐字节对齐：32 字节 PFH 头、deps@12、fcnt@16、idx_size@20；aligned 标志 = 第 4 字节 & 0x40
- db 表 4 种布局：STD(`fc fd fe ff`) / GUID+VER(`fd fe fc ff`+GUID+VER) / GUID+V0 / V0；5 字节且行数=0 视为合法空表
- used_mods.txt 为 UTF-16LE
- 科技表 schema：key/building_level(S16)、position_index/research_points_required/4×prestige/ai_bias/unique_index(I32)、mp_available_early/late(B)、icon_name/info_pic(S16)

## 环境依赖

- Rust stable-msvc + VS Build Tools(C++ 工作负载) + WebView2 Runtime（Win11 自带）
