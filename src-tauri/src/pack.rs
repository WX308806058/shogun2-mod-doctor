//! pack 文件（PFH 头）解析 —— 与 mod_doctor.py 的 read_pack 逐字节对齐

pub struct Entry {
    pub name: String,
    pub data: Vec<u8>,
    /// 数据越界等单项错误（包本身可读）
    pub err: Option<String>,
}

fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn u64le(b: &[u8], o: usize) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[o..o + 8]);
    u64::from_le_bytes(a)
}

/// 只读 8 字节判断 pack 类型；非 PFH 返回 None
pub fn pack_type(path: &std::path::Path) -> Option<u8> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut h = [0u8; 8];
    f.read_exact(&mut h).ok()?;
    if &h[0..3] != b"PFH" {
        return None;
    }
    Some(h[4])
}

/// 完整解析 pack，返回 (entries, 全局错误)。
/// entries 有值但 err 有值 = 索引部分截断（尽力解析）。
pub fn read_pack(path: &str) -> Result<(Vec<Entry>, Option<String>), String> {
    let raw = std::fs::read(path).map_err(|e| e.to_string())?;
    if raw.is_empty() {
        return Err("0 字节文件".into());
    }
    if raw.len() < 4 || &raw[0..3] != b"PFH" {
        let sig: Vec<u8> = raw.iter().take(4).copied().collect();
        return Err(format!("非法签名 {:?}", sig));
    }
    if raw.len() < 32 {
        return Err("头部不完整".into());
    }
    let _ptype = raw[4];
    let aligned = raw[3] & 0x40 != 0;
    let deps_sz = u32le(&raw, 12) as usize;
    let fcnt = u32le(&raw, 16) as usize;
    let isz = u32le(&raw, 20) as usize;
    let mut off = 32 + deps_sz;
    if off + isz > raw.len() {
        return Err(format!("索引越界 (idx@{}+{} > {})", off, isz, raw.len()));
    }
    let idx = raw[off..off + isz].to_vec();
    off += isz;

    let mut ents: Vec<Entry> = Vec::new();
    let mut o = 0usize;
    let mut cur = off;
    for i in 0..fcnt {
        if o + 4 > idx.len() {
            return Ok((ents, Some(format!("索引在第 {} 项截断", i))));
        }
        let fsz = u32le(&idx, o) as usize;
        o += 4;
        let foff;
        if aligned {
            if o + 12 > idx.len() {
                return Ok((ents, Some(format!("索引 aligned 字段在第 {} 项截断", i))));
            }
            foff = u64le(&idx, o) as usize;
            o += 12;
        } else {
            foff = cur;
        }
        let z = match idx[o..].iter().position(|&b| b == 0) {
            Some(z) => o + z,
            None => return Ok((ents, Some(format!("第 {} 项名字无结束符", i)))),
        };
        let nm = String::from_utf8_lossy(&idx[o..z]).into_owned();
        o = z + 1;
        if foff + fsz > raw.len() {
            ents.push(Entry {
                name: nm,
                data: Vec::new(),
                err: Some(format!("数据越界 off={} size={}", foff, fsz)),
            });
        } else {
            ents.push(Entry {
                name: nm,
                data: raw[foff..foff + fsz].to_vec(),
                err: None,
            });
        }
        if !aligned {
            cur += fsz;
        }
    }
    Ok((ents, None))
}
