//! db 表头分类 + 科技表 14 字段解析 —— 与 mod_doctor.py 对齐

pub const STD_M: [u8; 4] = [0xfc, 0xfd, 0xfe, 0xff];
pub const CE_M: [u8; 4] = [0xfd, 0xfe, 0xfc, 0xff];

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableInfo {
    /// STD | GUID+VER | GUID+V0 | V0 | EMPTY
    pub layout: String,
    pub ver: i32,
    pub rows: u32,
    pub start: usize,
}

fn u16le(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn i32le(b: &[u8], o: usize) -> i32 {
    i32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// 分类表头布局；None = 无法解析（真正的结构异常）
pub fn classify(blob: &[u8]) -> Option<TableInfo> {
    let n = blob.len();
    // 合法空表：mysterious_byte + u32 行数=0（原生 data.pack 中大量存在）
    if n == 5 {
        let cnt = u32le(blob, 1);
        return if cnt == 0 {
            Some(TableInfo { layout: "EMPTY".into(), ver: 0, rows: 0, start: 5 })
        } else {
            None
        };
    }
    if n < 13 {
        return None;
    }
    if blob[0..4] == CE_M {
        if n < 6 {
            return None;
        }
        let glen = u16le(blob, 4) as usize;
        if !(8..=80).contains(&glen) {
            return None;
        }
        let rest = 6 + glen * 2;
        if rest + 13 <= n && blob[rest..rest + 4] == STD_M {
            return Some(TableInfo {
                layout: "GUID+VER".into(),
                ver: i32le(blob, rest + 4),
                rows: u32le(blob, rest + 9),
                start: rest + 13,
            });
        }
        if rest + 5 <= n {
            return Some(TableInfo {
                layout: "GUID+V0".into(),
                ver: 0,
                rows: u32le(blob, rest + 1),
                start: rest + 5,
            });
        }
        return None;
    }
    if blob[0..4] == STD_M {
        return Some(TableInfo {
            layout: "STD".into(),
            ver: i32le(blob, 4),
            rows: u32le(blob, 9),
            start: 13,
        });
    }
    // V0：mysterious_byte + u32 行数
    let cnt = u32le(blob, 1);
    if cnt > 0 && (cnt as usize) < n * 4 {
        return Some(TableInfo { layout: "V0".into(), ver: 0, rows: cnt, start: 5 });
    }
    None
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TechRow {
    pub key: String,
    pub research_points_required: i32,
}

/// 科技表 14 字段 schema 顺序
const S16: u8 = 0;
const I32: u8 = 1;
const U8: u8 = 2;
const TECH_FIELDS: [(&str, u8); 14] = [
    ("key", S16),
    ("building_level", S16),
    ("position_index", I32),
    ("research_points_required", I32),
    ("icon_name", S16),
    ("military_prestige", I32),
    ("naval_prestige", I32),
    ("economic_prestige", I32),
    ("enlightenment_prestige", I32),
    ("mp_available_early", U8),
    ("mp_available_late", U8),
    ("info_pic", S16),
    ("ai_bias", I32),
    ("unique_index", I32),
];

/// 严格按 14 字段解析科技表；尾部残留也算错误
pub fn parse_tech_rows(blob: &[u8]) -> Result<Vec<TechRow>, String> {
    let info = classify(blob).ok_or("未知表头")?;
    let n = blob.len();
    let mut o = info.start;
    let mut rows: Vec<TechRow> = Vec::new();
    for i in 0..info.rows as usize {
        let mut key = String::new();
        let mut points = 0i32;
        for (fname, ftype) in TECH_FIELDS.iter() {
            match *ftype {
                S16 => {
                    if o + 2 > n {
                        return Err(format!("第 {} 行 {} 长度越界", i, fname));
                    }
                    let sl = u16le(blob, o) as usize;
                    o += 2;
                    if o + sl * 2 > n {
                        return Err(format!("第 {} 行 {} 字符串越界", i, fname));
                    }
                    let u: Vec<u16> = (0..sl).map(|k| u16le(blob, o + k * 2)).collect();
                    let s = String::from_utf16(&u).map_err(|_| format!("第 {} 行 {} UTF-16 非法", i, fname))?;
                    if *fname == "key" {
                        key = s;
                    }
                    o += sl * 2;
                }
                I32 => {
                    if o + 4 > n {
                        return Err(format!("第 {} 行 {} 越界", i, fname));
                    }
                    if *fname == "research_points_required" {
                        points = i32le(blob, o);
                    }
                    o += 4;
                }
                _ => {
                    if o + 1 > n {
                        return Err(format!("第 {} 行 {} 越界", i, fname));
                    }
                    o += 1;
                }
            }
        }
        rows.push(TechRow { key, research_points_required: points });
    }
    if o != n {
        return Err(format!("尾部残留 {} 字节", n - o));
    }
    Ok(rows)
}

/// 容错遍历 u16 长度前缀字符串（用于引用表内容扫描）
pub fn extract_strings(blob: &[u8]) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    let n = blob.len();
    let mut o = 0usize;
    while o + 2 <= n {
        let sl = u16le(blob, o) as usize;
        if (4..=100).contains(&sl) && o + 2 + sl * 2 <= n {
            let u: Vec<u16> = (0..sl).map(|k| u16le(blob, o + 2 + k * 2)).collect();
            if let Ok(s) = String::from_utf16(&u) {
                if s.chars().all(|c| !c.is_control()) {
                    out.insert(s);
                    o += 2 + sl * 2;
                    continue;
                }
            }
        }
        o += 2;
    }
    out
}
