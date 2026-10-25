#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod dbtable;
mod pack;
mod scan;

use tauri::{Emitter, Manager};

const DEFAULT_GAME: &str = "D:/Program Files/Steam/steamapps/common/Total War SHOGUN 2";

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
        .invoke_handler(tauri::generate_handler![default_game_dir, scan_game])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
