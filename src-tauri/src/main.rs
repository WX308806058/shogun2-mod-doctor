#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod dbtable;
mod pack;
mod scan;

use std::process::Command;
use tauri::{Emitter, Manager};

const DEFAULT_GAME: &str = "D:/Program Files/Steam/steamapps/common/Total War SHOGUN 2";
/// 发布功能使用的本机仓库路径（发布功能仅限开发机）
const PROJECT_DIR: &str = "D:/Shou2Mods/Shogun2ModDoctor";
const REPO_API: &str = "https://api.github.com/repos/WX308806058/shogun2-mod-doctor/releases/latest";
const PROXY: &str = "http://127.0.0.1:7890";

#[tauri::command]
fn default_game_dir() -> String {
    if std::path::Path::new(DEFAULT_GAME).join("data").exists() {
        DEFAULT_GAME.to_string()
    } else {
        String::new()
    }
}

#[tauri::command]
fn scan_game(
    app: tauri::AppHandle,
    game_dir: String,
    include_all: bool,
) -> Result<scan::ScanReport, String> {
    let handle = app.clone();
    let cb = move |cur: usize, total: usize, name: String| {
        let _ = handle.emit(
            "scan-progress",
            serde_json::json!({ "current": cur, "total": total, "name": name }),
        );
    };
    scan::do_scan(&game_dir, include_all, Some(&cb))
}

/// 在指定目录执行 git，proxy=true 时临时挂 Clash 代理
fn run_git(dir: &str, proxy: bool, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new("git");
    cmd.current_dir(dir);
    if proxy {
        cmd.arg("-c").arg(format!("http.proxy={}", PROXY));
        cmd.arg("-c").arg(format!("https.proxy={}", PROXY));
    }
    cmd.args(args);
    let out = cmd.output().map_err(|e| format!("启动 git 失败: {}", e))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

/// 检查更新：查 GitHub 最新 Release（curl 优先走代理，失败自动直连）
#[tauri::command]
fn check_update() -> Result<String, String> {
    let mut last_err = String::new();
    for use_proxy in [true, false] {
        let mut cmd = Command::new("curl");
        cmd.arg("-s").arg("-m").arg("20").arg("-L");
        if use_proxy {
            cmd.arg("--proxy").arg(PROXY);
        }
        cmd.arg("-H").arg("Accept: application/vnd.github+json");
        cmd.arg(REPO_API);
        if let Ok(out) = cmd.output() {
            if out.status.success() {
                let txt = String::from_utf8_lossy(&out.stdout);
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
                    if let (Some(tag), Some(url)) = (
                        v.get("tag_name").and_then(|x| x.as_str()),
                        v.get("html_url").and_then(|x| x.as_str()),
                    ) {
                        return Ok(serde_json::json!({ "tag": tag, "url": url }).to_string());
                    }
                }
                last_err = "响应解析失败".into();
                continue;
            }
            last_err = format!("curl 退出码 {:?}", out.status.code());
        } else {
            last_err = "curl 不可用".into();
        }
    }
    Err(format!("无法访问 GitHub（可尝试开启 Clash 后重试）。{}", last_err))
}

/// 用系统默认浏览器打开发布页（仅允许本仓库的链接）
#[tauri::command]
fn open_release_page(url: String) -> Result<(), String> {
    if !url.starts_with("https://github.com/WX308806058/shogun2-mod-doctor") {
        return Err("不允许打开外部链接".into());
    }
    Command::new("cmd")
        .args(["/C", "start", "", &url])
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 一键发布：commit（如有变更）→ 附注 tag → 推送 main 与 tag（走代理，失败直连重试）
#[tauri::command]
fn publish_release(version: String, notes: String, dry_run: bool) -> Result<String, String> {
    let v = version.trim().trim_start_matches(['v', 'V']).to_string();
    let parts: Vec<&str> = v.split('.').collect();
    if parts.len() != 3 || parts.iter().any(|s| s.parse::<u32>().is_err()) {
        return Err("版本号格式应为 x.y.z，例如 0.2.1".into());
    }
    if notes.trim().is_empty() {
        return Err("发布说明不能为空".into());
    }
    if !std::path::Path::new(PROJECT_DIR).join(".git").exists() {
        return Err("未找到本机仓库 D:/Shou2Mods/Shogun2ModDoctor（发布功能仅限开发机使用）".into());
    }
    let tag = format!("v{}", v);
    // tag 已存在则拒绝
    if run_git(PROJECT_DIR, false, &["rev-parse", "--verify", &format!("refs/tags/{}", tag)]).is_ok() {
        return Err(format!("标签 {} 已存在，请换一个版本号", tag));
    }
    // 暂存全部变更（无变更时 commit 会失败，忽略即可）
    run_git(PROJECT_DIR, false, &["add", "-A"])?;
    let commit_msg = format!("v{}: {}", v, notes);
    let committed = run_git(PROJECT_DIR, false, &["commit", "-m", &commit_msg]).is_ok();
    if dry_run {
        let status = run_git(PROJECT_DIR, false, &["status", "--short"])?;
        return Ok(format!(
            "试运行通过。已暂存变更（{}提交 commit）。正式发布会创建标签 {} 并推送 main + {}（走代理 {}）。\n当前未提交内容：{}",
            if committed { "已" } else { "无变更，跳过" }, tag, tag, PROXY,
            if status.trim().is_empty() { "无".into() } else { status })
        );
    }
    let _ = run_git(PROJECT_DIR, false, &["commit", "-m", &commit_msg]);
    run_git(PROJECT_DIR, false, &["tag", "-a", &tag, "-m", notes.trim()])?;
    // 推送：代理优先，失败直连重试
    let push_main = run_git(PROJECT_DIR, true, &["push", "origin", "main"])
        .or_else(|_| run_git(PROJECT_DIR, false, &["push", "origin", "main"]));
    let push_tag = run_git(PROJECT_DIR, true, &["push", "origin", &tag])
        .or_else(|_| run_git(PROJECT_DIR, false, &["push", "origin", &tag]));
    match (push_main, push_tag) {
        (Ok(_), Ok(_)) => Ok(format!(
            "已推送 {}。GitHub Actions 云端构建约 10 分钟，完成后可在 Releases 页下载安装包。",
            tag
        )),
        (e1, e2) => {
            let _ = run_git(PROJECT_DIR, false, &["tag", "-d", &tag]); // 推送失败撤销本地 tag
            Err(format!(
                "推送失败（请确认 Clash 已开启）。\nmain: {}\ntag: {}",
                e1.map(|_| "OK".into()).unwrap_or_else(|e| e),
                e2.map(|_| "OK".into()).unwrap_or_else(|e| e)
            ))
        }
    }
}

fn main() {
    // CLI 模式: shogun2-mod-doctor.exe scan <游戏目录> [--all] → 输出 JSON
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 && args[1] == "scan" {
        let dir = args.get(2).cloned().unwrap_or_else(|| DEFAULT_GAME.to_string());
        let all = args.iter().any(|a| a == "--all");
        match scan::do_scan(&dir, all, None) {
            Ok(rep) => println!("{}", serde_json::to_string_pretty(&rep).unwrap()),
            Err(e) => {
                eprintln!("扫描失败: {}", e);
                std::process::exit(1);
            }
        }
        return;
    }
    tauri::Builder::default()
        .setup(|app| {
            #[cfg(debug_assertions)]
            {
                if let Some(w) = app.get_webview_window("main") {
                    w.open_devtools();
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            default_game_dir,
            scan_game,
            check_update,
            open_release_page,
            publish_release
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
