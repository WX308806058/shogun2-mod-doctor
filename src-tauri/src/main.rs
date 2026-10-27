#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod dbtable;
mod pack;
mod scan;

use std::process::Command;
use tauri::{Emitter, Manager};

const DEFAULT_GAME: &str = "D:/Program Files/Steam/steamapps/common/Total War SHOGUN 2";
/// 发布功能使用的本机仓库路径（发布功能仅限开发机）
const PROJECT_DIR: &str = "D:/JetBrains/ViteProject/Shogun2ModDoctor";
const REPO_API: &str = "https://api.github.com/repos/WX308806058/shogun2-mod-doctor/releases/latest";
const REPO_BASE: &str = "https://github.com/WX308806058/shogun2-mod-doctor";
const PROXY: &str = "http://127.0.0.1:7890";

/// GitHub API 请求：curl 优先走代理，失败自动直连；带认证时限额从 60/h 提升到 5000/h。
/// 返回 (HTTP 状态码, 响应体)
fn gh_api(path: &str) -> Result<(u32, String), String> {
    // 一次性读取本机 git 凭据里的 GitHub token（无则匿名请求）
    static GH_TOKEN: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    let token = GH_TOKEN.get_or_init(|| {
        use std::io::Write;
        let child = Command::new("git")
            .args(["credential", "fill"])
            .env("GCM_INTERACTIVE", "never")
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(_) => return None,
        };
        if let Some(mut sin) = child.stdin.take() {
            let _ = sin.write_all(b"protocol=https\nhost=github.com\n\n");
        }
        if let Ok(o) = child.wait_with_output() {
            let txt = String::from_utf8_lossy(&o.stdout);
            for line in txt.lines() {
                if let Some(pw) = line.strip_prefix("password=") {
                    let t = pw.trim().to_string();
                    if !t.is_empty() {
                        return Some(t);
                    }
                }
            }
        }
        None
    });

    for use_proxy in [true, false] {
        let mut cmd = Command::new("curl");
        hide_window(&mut cmd);
        cmd.arg("-s").arg("-m").arg("20").arg("-L");
        cmd.arg("-w").arg("\n__GH_CODE__%{http_code}");
        if use_proxy {
            cmd.arg("--proxy").arg(PROXY);
        }
        cmd.arg("-H").arg("Accept: application/vnd.github+json");
        if let Some(tok) = token {
            cmd.arg("-H").arg(format!("Authorization: Bearer {}", tok));
        }
        cmd.arg(format!("https://api.github.com{}", path));
        if let Ok(out) = cmd.output() {
            if !out.status.success() {
                continue;
            }
            let txt = String::from_utf8_lossy(&out.stdout).into_owned();
            if let Some(pos) = txt.find("\n__GH_CODE__") {
                let code: u32 = txt[pos + 12..].trim().parse().unwrap_or(0);
                let body = txt[..pos].to_string();
                if !body.is_empty() {
                    return Ok((code, body));
                }
            }
        }
    }
    Err("无法访问 GitHub API（可尝试开启 Clash 后重试）".into())
}

/// gh_api + JSON 解析 + 常见错误翻译（限流 / 404 / 非 JSON）
fn gh_json(path: &str) -> Result<serde_json::Value, String> {
    let (code, body) = gh_api(path)?;
    let v: serde_json::Value = serde_json::from_str(&body).map_err(|_| {
        format!(
            "GitHub 返回非 JSON（HTTP {}）：{}",
            code,
            body.chars().take(120).collect::<String>()
        )
    })?;
    // releases/latest 无 Release 时返回 404 + message
    if code != 200 && v.get("message").is_some() {
        let msg = v["message"].as_str().unwrap_or("").to_string();
        if code == 403 && msg.contains("rate limit") {
            return Err("GitHub API 匿名限流（60 次/小时/IP），请稍后再试或开启代理更换出口".into());
        }
        if code == 404 {
            return Err("该仓库还没有发布过 Release".into());
        }
        return Err(format!("GitHub API 错误（HTTP {}）：{}", code, msg));
    }
    Ok(v)
}

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

/// GUI 程序启动控制台子进程（git/curl/cmd）时隐藏弹出的命令窗口
#[cfg(windows)]
fn hide_window(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}
#[cfg(not(windows))]
fn hide_window(_cmd: &mut Command) {}

/// 在指定目录执行 git，proxy=true 时临时挂 Clash 代理
fn run_git(dir: &str, proxy: bool, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new("git");
    cmd.current_dir(dir);
    hide_window(&mut cmd);
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

/// 检查更新：查 GitHub 最新 Release，返回 tag / 发布页 / 安装包直链
#[tauri::command]
fn check_update() -> Result<String, String> {
    let v = gh_json("/repos/WX308806058/shogun2-mod-doctor/releases/latest")?;
    let tag = v
        .get("tag_name")
        .and_then(|x| x.as_str())
        .ok_or("响应解析失败")?
        .to_string();
    let url = v
        .get("html_url")
        .and_then(|x| x.as_str())
        .ok_or("响应解析失败")?
        .to_string();
    // 找 setup.exe 资产的直链
    let asset = v["assets"]
        .as_array()
        .and_then(|arr| {
            arr.iter().find(|a| {
                a["name"]
                    .as_str()
                    .map(|n| n.starts_with("Shogun2ModDoctor") && n.ends_with("-setup.exe"))
                    .unwrap_or(false)
            })
        })
        .and_then(|a| a["browser_download_url"].as_str())
        .unwrap_or("")
        .to_string();
    Ok(serde_json::json!({ "tag": tag, "url": url, "asset": asset }).to_string())
}

/// 下载新版安装包到临时目录（gh-proxy 镜像优先，其次直链+代理，最后直链直连）
#[tauri::command]
fn download_update(asset_url: String) -> Result<String, String> {
    if !asset_url.starts_with("https://github.com/WX308806058/shogun2-mod-doctor/releases/download/") {
        return Err("不允许的下载地址".into());
    }
    let dest = format!(
        "{}\\Shogun2ModDoctor_update_setup.exe",
        std::env::temp_dir().display()
    );
    let sources = [
        format!("https://gh-proxy.com/{}", asset_url),
        asset_url.clone(),
    ];
    let mut last_err = String::new();
    for u in &sources {
        for use_proxy in [true, false] {
            let mut cmd = Command::new("curl");
            hide_window(&mut cmd);
            cmd.arg("-sSL")
                .arg("--retry")
                .arg("2")
                .arg("-m")
                .arg("600")
                .arg("-o")
                .arg(&dest);
            if use_proxy {
                cmd.arg("--proxy").arg(PROXY);
            }
            cmd.arg(u);
            if let Ok(out) = cmd.output() {
                if out.status.success() {
                    if let Ok(md) = std::fs::metadata(&dest) {
                        if md.len() > 1_000_000 {
                            return Ok(dest);
                        }
                    }
                }
            }
            last_err = format!("源 {}（{}）下载失败", u, if use_proxy { "代理" } else { "直连" });
        }
    }
    Err(format!("下载失败（可尝试开启 Clash）。{}", last_err))
}

/// 运行已下载的安装包静默安装；应用随即退出，装完从桌面快捷方式重新打开即为新版
#[tauri::command]
fn install_update(path: String) -> Result<(), String> {
    if !path.contains("Shogun2ModDoctor_update_setup.exe") || !std::path::Path::new(&path).exists() {
        return Err("安装包不存在".into());
    }
    Command::new(&path)
        .arg("/S")
        .spawn()
        .map_err(|e| format!("启动安装程序失败: {}", e))?;
    std::process::exit(0);
}

/// 查询某 tag 的云端构建状态（GitHub Actions）
#[tauri::command]
fn ci_status(tag: String) -> Result<String, String> {
    let v = gh_json("/repos/WX308806058/shogun2-mod-doctor/actions/runs?per_page=15")?;
    for run in v["workflow_runs"].as_array().into_iter().flatten() {
        if run["head_branch"].as_str() == Some(tag.as_str()) {
            let status = run["status"].as_str().unwrap_or("?").to_string();
            let conclusion = run["conclusion"].as_str().unwrap_or("").to_string();
            return Ok(
                serde_json::json!({ "status": status, "conclusion": conclusion }).to_string()
            );
        }
    }
    Ok("{\"status\":\"not_found\",\"conclusion\":\"\"}".into())
}

/// 该 tag 的 Release 是否已发布（资产是否挂上）；404 = 尚未发布
#[tauri::command]
fn release_ready(tag: String) -> Result<bool, String> {
    match gh_api(&format!(
        "/repos/WX308806058/shogun2-mod-doctor/releases/tags/{}",
        tag
    )) {
        Ok((_, body)) => {
            let v: serde_json::Value = serde_json::from_str(&body)
                .map_err(|e| format!("响应解析失败: {}", e))?;
            Ok(v.get("assets")
                .map(|a| a.as_array().map(|x| !x.is_empty()).unwrap_or(false))
                .unwrap_or(false))
        }
        Err(e) if e.contains("404") => Ok(false),
        Err(e) => Err(e),
    }
}

/// 用系统默认浏览器打开发布页（仅允许本仓库的链接）
#[tauri::command]
fn open_release_page(url: String) -> Result<(), String> {
    if !url.starts_with("https://github.com/WX308806058/shogun2-mod-doctor") {
        return Err("不允许打开外部链接".into());
    }
    let mut cmd = Command::new("cmd");
    hide_window(&mut cmd);
    cmd.args(["/C", "start", "", &url])
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RepoStatus {
    project_dir: String,
    latest_tag: String,
    branch: String,
    dirty: u32,
    next_version: String,
}

/// 仓库状态：最新标签 / 当前分支 / 工作区改动数 / 建议的下一版本
#[tauri::command]
fn repo_status() -> Result<RepoStatus, String> {
    if !std::path::Path::new(PROJECT_DIR).join(".git").exists() {
        return Err(format!("未找到本机仓库 {}", PROJECT_DIR));
    }
    let tags = run_git(PROJECT_DIR, false, &["tag", "-l", "--sort=-v:refname"])
        .map_err(|e| format!("读取标签失败: {}", e))?;
    let latest_tag = tags.lines().next().unwrap_or("").trim().to_string();
    let branch = run_git(PROJECT_DIR, false, &["branch", "--show-current"])
        .map_err(|e| e.to_string())?
        .trim()
        .to_string();
    let st = run_git(PROJECT_DIR, false, &["status", "--porcelain"]).map_err(|e| e.to_string())?;
    let dirty = st.lines().filter(|l| !l.trim().is_empty()).count() as u32;
    // 建议版本 = 最新标签 patch + 1
    let next_version = {
        let v = latest_tag.trim_start_matches('v');
        let parts: Vec<u32> = v.split('.').filter_map(|s| s.parse().ok()).collect();
        if parts.len() == 3 {
            format!("{}.{}.{}", parts[0], parts[1], parts[2] + 1)
        } else {
            "0.1.0".into()
        }
    };
    Ok(RepoStatus { project_dir: PROJECT_DIR.into(), latest_tag, branch, dirty, next_version })
}

/// 一键发布：commit（如有变更）→ 附注 tag → 推送 main 与 tag（走代理，失败直连重试）
/// 每一步通过 publish-log 事件推送带序号的日志行
#[tauri::command]
fn publish_release(
    app: tauri::AppHandle,
    version: String,
    commit_msg: String,
    notes: String,
    dry_run: bool,
) -> Result<String, String> {
    let handle = app.clone();
    let log = move |msg: &str| {
        let _ = handle.emit("publish-log", msg);
    };
    let v = version.trim().trim_start_matches(['v', 'V']).to_string();
    let parts: Vec<&str> = v.split('.').collect();
    if parts.len() != 3 || parts.iter().any(|s| s.parse::<u32>().is_err()) {
        return Err("版本号格式应为 x.y.z，例如 0.2.2".into());
    }
    if commit_msg.trim().is_empty() {
        return Err("提交说明不能为空".into());
    }
    if !std::path::Path::new(PROJECT_DIR).join(".git").exists() {
        return Err(format!("未找到本机仓库 {}（发布功能仅限开发机使用）", PROJECT_DIR));
    }
    let tag = format!("v{}", v);
    log(&format!("检查标签 {} 是否已存在…", tag));
    if run_git(PROJECT_DIR, false, &["rev-parse", "--verify", &format!("refs/tags/{}", tag)]).is_ok() {
        return Err(format!("标签 {} 已存在，请换一个版本号", tag));
    }
    log("暂存工作区变更…");
    run_git(PROJECT_DIR, false, &["add", "-A"])?;
    let st = run_git(PROJECT_DIR, false, &["status", "--porcelain"]).unwrap_or_default();
    let staged = st.lines().filter(|l| !l.trim().is_empty()).count();
    log(&format!("已暂存 {} 处变更", staged));
    if dry_run {
        log("试运行结束：未创建标签、未推送。");
        return Ok(format!("试运行通过：将创建标签 {} 并推送 main + {}。", tag, tag));
    }
    log(&format!("提交：{}", commit_msg.trim()));
    if run_git(PROJECT_DIR, false, &["commit", "-m", commit_msg.trim()]).is_ok() {
        log("提交完成");
    } else {
        log("无新变更，跳过 commit");
    }
    log(&format!("创建附注标签 {}…", tag));
    run_git(PROJECT_DIR, false, &["tag", "-a", &tag, "-m", notes.trim()])?;
    log("推送 main（优先走 Clash 代理）…");
    let pm = run_git(PROJECT_DIR, true, &["push", "origin", "main"])
        .or_else(|_| { log("代理失败，改直连重试…"); run_git(PROJECT_DIR, false, &["push", "origin", "main"]) });
    log(&format!("推送 main：{}", if pm.is_ok() { "成功" } else { "失败" }));
    log(&format!("推送标签 {}…", tag));
    let pt = run_git(PROJECT_DIR, true, &["push", "origin", &tag])
        .or_else(|_| { log("代理失败，改直连重试…"); run_git(PROJECT_DIR, false, &["push", "origin", &tag]) });
    match (pm, pt) {
        (Ok(_), Ok(_)) => {
            log(&format!("全部推送成功。GitHub Actions 构建约 10 分钟，随后可在 Releases 页下载 {} 安装包。", tag));
            Ok(format!("{} 发布完成。", tag))
        }
        (e1, e2) => {
            let _ = run_git(PROJECT_DIR, false, &["tag", "-d", &tag]);
            log(&format!("推送失败，已撤销本地标签 {}。", tag));
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
            download_update,
            install_update,
            open_release_page,
            repo_status,
            ci_status,
            release_ready,
            publish_release
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
