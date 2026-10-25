//! 扫描主逻辑：加载顺序 → 同名表覆盖链 → 科技生态悬空引用

use crate::dbtable::{classify, extract_strings, parse_tech_rows, TechRow};
use crate::pack::{pack_type, read_pack};
use regex::Regex;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub const TECH_TABLE: &str = r"db\technologies_tables\technologies";
const TECH_REF_TABLES: [&str; 5] = [
    r"db\technology_effects_junction_tables\technology_effects_junction",
    r"db\campaign_ai_technology_path_junctions_tables\campaign_ai_technology_path_junctions",
    r"db\building_level_required_technology_junctions_tables\building_level_required_technology_junctions",
    r"db\technology_nodes_tables\technology_nodes",
    r"db\unit_required_technology_junctions_tables\unit_required_technology_junctions",
];

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub level: String, // ERROR | WARN | INFO | OK
    pub pack: String,
    pub msg: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConflictRow {
    pub level: String,
    pub chain: Vec<String>,
    pub table: String,
    pub note: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SimpleItem {
    pub level: String,
    pub msg: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub scanned_at: String,
    pub game_dir: String,
    pub packs_total: usize,
    pub packs_ok: usize,
    pub packs_bad: usize,
    pub mods_enabled: usize,
    pub errors: usize,
    pub warnings: usize,
    pub findings: Vec<Finding>,
    pub conflicts: Vec<ConflictRow>,
    pub tech: Vec<SimpleItem>,
}

struct ChainEntry {
    pack: String,
    ptype: String,
    rank: u32,
    layout: String,
    ver: i32,
    rows: u32,
    blob: Vec<u8>,
}

/// 加载项：rank 越大 = 优先级越高 = 实际生效（幕府2：movie > mod > patch > release > boot；
/// mod 档内 used_mods.txt 越靠前优先级越高；movie 档内按文件名升序，越靠后越高）
pub struct LoadItem {
    pub path: String,
    pub tag: String,
    pub rank: u32,
}

struct FileHit {
    pack: String,
    ptype: String,
    rank: u32,
}

fn decode_utf16(raw: &[u8]) -> String {
    let u: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    String::from_utf16_lossy(&u)
}

/// 解析 used_mods.txt → (启用 mod 列表, 附加工作目录列表)
fn read_used_mods(game: &str) -> (Option<Vec<String>>, Vec<String>) {
    let p = Path::new(game).join("used_mods.txt");
    if !p.exists() {
        return (None, vec![]);
    }
    match std::fs::read(&p) {
        Ok(raw) => {
            let txt = decode_utf16(&raw);
            let re_mod = Regex::new(r#"mod\s+"([^"]+)""#).unwrap();
            let re_dir = Regex::new(r#"add_working_directory\s+"([^"]+)""#).unwrap();
            let mods: Vec<String> = re_mod.captures_iter(&txt).map(|c| c[1].to_string()).collect();
            let dirs: Vec<String> = re_dir.captures_iter(&txt).map(|c| c[1].to_string()).collect();
            (Some(mods), dirs)
        }
        Err(_) => (None, vec![]),
    }
}

/// 按真实加载顺序收集 pack → LoadItem（path / 类型标签 / 优先级 rank）
fn build_load_order(game: &str, include_all: bool) -> Vec<LoadItem> {
    let data_dir = Path::new(game).join("data");
    let (used_opt, wdirs) = read_used_mods(game);
    let used = used_opt.unwrap_or_default();
    // 附加工作目录（Steam 创意工坊目录等）中的 pack
    let mut ws_packs: HashMap<String, String> = HashMap::new();
    for d in &wdirs {
        if let Ok(rd) = std::fs::read_dir(d) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().map(|x| x.eq_ignore_ascii_case("pack")).unwrap_or(false) {
                    if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                        ws_packs.insert(name.to_lowercase(), p.to_string_lossy().into_owned());
                    }
                }
            }
        }
    }
    let mut boot: Vec<String> = vec![];
    let mut release: Vec<String> = vec![];
    let mut patch: Vec<String> = vec![];
    let mut movie: Vec<String> = vec![];
    let mut data_mods: Vec<String> = vec![];
    if let Ok(rd) = std::fs::read_dir(&data_dir) {
        let mut names: Vec<String> = rd
            .flatten()
            .filter(|e| e.path().extension().map(|x| x.eq_ignore_ascii_case("pack")).unwrap_or(false))
            .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
            .collect();
        names.sort_by_key(|a| a.to_lowercase());
        for fnm in names {
            let p = data_dir.join(&fnm);
            let t = match pack_type(&p) {
                Some(t) => t,
                None => continue,
            };
            let full = p.to_string_lossy().into_owned();
            match t {
                4 => movie.push(full),
                0 => boot.push(full),
                1 => release.push(full),
                2 => patch.push(full),
                3 => data_mods.push(full),
                _ => {}
            }
        }
    }
    // mod 档：严格按 used_mods.txt 顺序解析（靠前 = 优先级高）；--all 时补入其余 mod 包
    let mut mods: Vec<String> = vec![];
    let mut seen: HashSet<String> = HashSet::new();
    for m in &used {
        let p = data_dir.join(m);
        let resolved = if p.exists() {
            Some(p.to_string_lossy().into_owned())
        } else {
            ws_packs.get(&m.to_lowercase()).cloned()
        };
        if let Some(path) = resolved {
            if seen.insert(path.to_lowercase()) {
                mods.push(path);
            }
        }
    }
    if include_all {
        for p in data_mods {
            if seen.insert(p.to_lowercase()) {
                mods.push(p);
            }
        }
    }
    let mut order: Vec<LoadItem> = Vec::new();
    for (i, p) in boot.iter().enumerate() {
        order.push(LoadItem { path: p.clone(), tag: "boot".into(), rank: i as u32 });
    }
    for (i, p) in release.iter().enumerate() {
        order.push(LoadItem { path: p.clone(), tag: "release".into(), rank: 10_000 + i as u32 });
    }
    for (i, p) in patch.iter().enumerate() {
        order.push(LoadItem { path: p.clone(), tag: "patch".into(), rank: 20_000 + i as u32 });
    }
    for (i, p) in mods.iter().enumerate() {
        // 越靠前优先级越高：rank 递减
        let rank = 30_000 + (2000u32.saturating_sub(i as u32));
        order.push(LoadItem { path: p.clone(), tag: "mod".into(), rank });
    }
    for (i, p) in movie.iter().enumerate() {
        order.push(LoadItem { path: p.clone(), tag: "movie".into(), rank: 40_000 + i as u32 });
    }
    order
}

fn now_string() -> String {
    // 简单本地时间（不引 chrono，用 std）：
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // 北京时间 = UTC+8
    let days = secs / 86400;
    let rem = secs % 86400 + 8 * 3600;
    let (h, mi, s) = ((rem / 3600) % 24, (rem % 3600) / 60, rem % 60);
    // 民用日期算法（自 1970-01-01）
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", y, m, d, h, mi, s)
}

pub fn do_scan(
    game: &str,
    include_all: bool,
    progress: Option<&dyn Fn(usize, usize, String)>,
) -> Result<ScanReport, String> {
    if !Path::new(game).join("data").exists() {
        return Err(format!("游戏 data 目录不存在: {}", game));
    }
    let order = build_load_order(game, include_all);
    let (_, wdirs) = read_used_mods(game);
    let (used, _) = read_used_mods(game);
    let mods_enabled = used.map(|u| u.len()).unwrap_or(0);
    let _ = wdirs;

    let mut findings: Vec<Finding> = Vec::new();
    let mut override_map: HashMap<String, Vec<ChainEntry>> = HashMap::new();
    let mut override_order: Vec<String> = Vec::new();
    let mut file_map: HashMap<String, Vec<FileHit>> = HashMap::new();
    let mut file_order: Vec<String> = Vec::new();
    let mut tech_native: Option<Vec<u8>> = None;
    let mut tech_winner: Option<(String, Vec<u8>, u32)> = None;
    let mut packs_ok = 0usize;
    let mut packs_bad = 0usize;

    for (pi, item) in order.iter().enumerate() {
        let p = &item.path;
        let ptype = &item.tag;
        let rank = item.rank;
        let base = Path::new(p).file_name().unwrap().to_string_lossy().into_owned();
        if let Some(cb) = progress {
            cb(pi + 1, order.len(), base.clone());
        }
        let (ents, err) = match read_pack(p) {
            Ok(v) => v,
            Err(e) => {
                findings.push(Finding { level: "ERROR".into(), pack: base.clone(), msg: format!("包不可读: {}", e) });
                packs_bad += 1;
                continue;
            }
        };
        match &err {
            Some(e) => {
                findings.push(Finding { level: "WARN".into(), pack: base.clone(), msg: e.clone() });
                packs_bad += 1;
            }
            None => packs_ok += 1,
        }
        if base.eq_ignore_ascii_case("data.pack") {
            for e in &ents {
                if e.name.eq_ignore_ascii_case(TECH_TABLE) {
                    tech_native = Some(e.data.clone());
                }
            }
        }
        for e in &ents {
            if let Some(ie) = &e.err {
                findings.push(Finding {
                    level: "ERROR".into(),
                    pack: base.clone(),
                    msg: format!("{}: {}", e.name, ie),
                });
                continue;
            }
            // 记录非 db 文件（campaign / 脚本 / 模型 / UI 等）的覆盖关系
            let fkey = e.name.to_lowercase();
            if !file_map.contains_key(&fkey) {
                file_order.push(fkey.clone());
            }
            file_map.entry(fkey).or_default().push(FileHit {
                pack: base.clone(),
                ptype: ptype.clone(),
                rank,
            });

            if !e.name.to_lowercase().starts_with("db") {
                continue;
            }
            let info = match classify(&e.data) {
                Some(i) => i,
                None => {
                    findings.push(Finding {
                        level: "ERROR".into(),
                        pack: base.clone(),
                        msg: format!("表头无法解析: {} (len={})", e.name, e.data.len()),
                    });
                    continue;
                }
            };
            if info.rows == 0 && (ptype == "mod" || ptype == "movie") {
                findings.push(Finding {
                    level: "WARN".into(),
                    pack: base.clone(),
                    msg: format!(
                        "空表(0行): {} —— 该表会整体覆盖原生表并清空全部原生行，极易造成悬空引用闪退",
                        e.name
                    ),
                });
            }
            let key = e.name.to_lowercase();
            if !override_map.contains_key(&key) {
                override_order.push(key.clone());
            }
            override_map.entry(key.clone()).or_default().push(ChainEntry {
                pack: base.clone(),
                ptype: ptype.clone(),
                rank,
                layout: info.layout,
                ver: info.ver,
                rows: info.rows,
                blob: e.data.clone(),
            });
            if key == TECH_TABLE.to_lowercase() {
                // 同一张表可能被多个包覆盖，取优先级最高（rank 最大）的那个作为实际生效表
                let better = match &tech_winner {
                    Some((_, _, r)) => rank > *r,
                    None => true,
                };
                if better {
                    tech_winner = Some((base.clone(), e.data.clone(), rank));
                }
            }
        }
    }

    // --- 同表多包覆盖（db 表） ---
    let mut conflicts: Vec<ConflictRow> = Vec::new();
    for key in &override_order {
        let chain = &override_map[key];
        if chain.len() > 1 {
            let mut layouts: Vec<String> = Vec::new();
            let mut inconsistent = false;
            for c in chain {
                let s = format!("{}({} v{})", c.pack, c.layout, c.ver);
                if !layouts.contains(&s) {
                    layouts.push(s);
                }
            }
            let layout_kinds: HashSet<&str> = chain.iter().map(|c| c.layout.as_str()).collect();
            let ver_set: HashSet<i32> = chain.iter().map(|c| c.ver).collect();
            if layout_kinds.len() > 1 || ver_set.len() > 1 {
                inconsistent = true;
            }
            // 幕府2：优先级最高者生效（不是最后加载者）
            let winner = chain.iter().max_by_key(|c| c.rank).unwrap();
            let mut note = format!(" → 赢家 {} ({} v{})", winner.pack, winner.layout, winner.ver);
            if inconsistent {
                note.push_str(&format!("  ⚠ 版本/布局不一致: {}", layouts.join(", ")));
            }
            conflicts.push(ConflictRow {
                level: if inconsistent { "WARN".into() } else { "INFO".into() },
                chain: chain.iter().map(|c| c.pack.clone()).collect(),
                table: key.replace('\\', "/"),
                note,
            });
        }
    }

    // --- 非 db 文件覆盖（campaign 脚本 / startpos.esf / 模型 / UI） ---
    let critical = |path: &str| -> bool {
        path.contains("startpos")
            || path.ends_with("scripting.lua")
            || path.starts_with("campaigns\\")
            || path.starts_with("scripts\\")
    };
    let mut file_conflicts = 0usize;
    for key in &file_order {
        if file_conflicts >= 80 {
            break;
        }
        // db 表已由上面按表粒度处理；Rusty PFM 的元数据文件无意义
        if key.starts_with("db\\") || key.ends_with(".rpfm_reserved") {
            continue;
        }
        let hits = &file_map[key];
        if hits.len() < 2 {
            continue;
        }
        let mut modhits: Vec<&FileHit> = hits.iter().filter(|h| h.ptype == "mod" || h.ptype == "movie").collect();
        if modhits.is_empty() {
            continue;
        }
        modhits.sort_by_key(|h| h.rank);
        let winner = *modhits.last().unwrap();
        let mod_names: Vec<String> = modhits.iter().map(|h| h.pack.clone()).collect();
        if modhits.len() >= 2 {
            // 真·mod 之间的文件级冲突
            conflicts.push(ConflictRow {
                level: "WARN".into(),
                chain: mod_names,
                table: key.replace('\\', "/"),
                note: format!(
                    " → 赢家 {}　该文件为整文件覆盖，其余 mod 对它的修改全部失效（{}）",
                    winner.pack,
                    if critical(key) { "campaign 级文件，影响存档与新战役" } else { "资源文件" }
                ),
            });
            file_conflicts += 1;
        } else if critical(key) {
            // 单个 mod 覆盖 campaign 级文件（替换原生），提示影响面
            let native: Vec<String> = hits.iter().filter(|h| h.ptype != "mod" && h.ptype != "movie").map(|h| h.pack.clone()).collect();
            if !native.is_empty() {
                conflicts.push(ConflictRow {
                    level: "INFO".into(),
                    chain: mod_names,
                    table: key.replace('\\', "/"),
                    note: format!(" → 覆盖原生 {}　campaign 级文件，需开新战役才完全生效", native.join(" / ")),
                });
                file_conflicts += 1;
            }
        }
    }
    let lvl_rank = |l: &str| match l { "ERROR" => 0, "WARN" => 1, _ => 2 };
    conflicts.sort_by_key(|c| lvl_rank(&c.level));

    // --- 科技生态完整性 ---
    let mut tech: Vec<SimpleItem> = Vec::new();
    let tech_key_re = Regex::new(r"^(?:boshin|gempei|shogun)_[a-z0-9_]+$").unwrap();
    if let (Some((wpack, wblob, _)), Some(native_blob)) = (&tech_winner, &tech_native) {
        let mut rows: Vec<TechRow> = Vec::new();
        let err_msg: Option<String> = match parse_tech_rows(wblob) {
            Ok(r) => {
                rows = r;
                None
            }
            Err(e) => Some(e),
        };
        let keys: Vec<&str> = rows.iter().map(|r| r.key.as_str()).collect();
        let keyset: HashSet<&str> = keys.iter().copied().collect();
        let mut dup = keys.clone();
        dup.sort_unstable();
        dup.dedup();
        let dup_count = keys.len() - dup.len();
        let native_rows: Vec<TechRow> = parse_tech_rows(native_blob).unwrap_or_default();
        let missing: Vec<&str> = native_rows
            .iter()
            .map(|r| r.key.as_str())
            .filter(|k| !keyset.contains(k))
            .collect();
        if let Some(ref e) = err_msg {
            tech.push(SimpleItem { level: "ERROR".into(), msg: format!("赢家表 {} 解析失败: {}", wpack, e) });
        }
        if dup_count > 0 {
            tech.push(SimpleItem {
                level: "ERROR".into(),
                msg: format!("赢家表 {} 存在 {} 个重复 key（游戏行为未定义）", wpack, dup_count),
            });
        }
        if !missing.is_empty() {
            let show: Vec<&str> = missing.iter().take(5).copied().collect();
            tech.push(SimpleItem {
                level: "ERROR".into(),
                msg: format!("赢家表 {} 缺少 {} 个原生 key: {}…", wpack, missing.len(), show.join(", ")),
            });
        }
        if err_msg.is_none() && dup_count == 0 && missing.is_empty() {
            tech.push(SimpleItem {
                level: "OK".into(),
                msg: format!("赢家表 {}: {} 行、无重复、无缺失", wpack, keys.len()),
            });
        }
        // 引用表悬空检查
        for ref_table in TECH_REF_TABLES.iter() {
            let rkey = ref_table.to_lowercase();
            let chain = match override_map.get(&rkey) {
                Some(c) if !c.is_empty() => c,
                _ => continue,
            };
            let last = chain.iter().max_by_key(|c| c.rank).unwrap();
            let strs = extract_strings(&last.blob);
            let mut cands: Vec<String> = strs
                .into_iter()
                .filter(|s| tech_key_re.is_match(s))
                .collect();
            if ref_table.ends_with("technology_nodes") {
                cands.retain(|s| !s.contains("_node_") && !s.ends_with("_main_civil") && !s.ends_with("_main_military"));
            }
            let dangling: Vec<String> = cands.into_iter().filter(|s| !keyset.contains(s.as_str())).collect();
            if dangling.is_empty() {
                tech.push(SimpleItem {
                    level: "OK".into(),
                    msg: format!("{}（包 {}）引用完整", ref_table.replace('\\', "/"), last.pack),
                });
            } else {
                let mut sorted = dangling.clone();
                sorted.sort();
                let show: Vec<String> = sorted.iter().take(4).cloned().collect();
                tech.push(SimpleItem {
                    level: "ERROR".into(),
                    msg: format!(
                        "悬空引用: {}（包 {}）引用了 {} 个不存在的科技 key，例如 {} → 加载页闪退高发",
                        ref_table.replace('\\', "/"),
                        last.pack,
                        dangling.len(),
                        show.join(", ")
                    ),
                });
            }
        }
    } else if tech_winner.is_none() {
        tech.push(SimpleItem {
            level: "WARN".into(),
            msg: "未找到科技表（无 mod 覆盖即使用原生表，属正常；但 data.pack 未识别到则为异常）".into(),
        });
    }

    let errors = findings.iter().filter(|f| f.level == "ERROR").count()
        + tech.iter().filter(|t| t.level == "ERROR").count();
    let warnings = findings.iter().filter(|f| f.level == "WARN").count()
        + conflicts.iter().filter(|c| c.level == "WARN").count()
        + tech.iter().filter(|t| t.level == "WARN").count();

    Ok(ScanReport {
        scanned_at: now_string(),
        game_dir: game.into(),
        packs_total: order.len(),
        packs_ok,
        packs_bad,
        mods_enabled,
        errors,
        warnings,
        findings,
        conflicts,
        tech,
    })
}
