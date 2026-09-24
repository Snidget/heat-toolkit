//! Работа с материалами HEAT3: извлечение имён из скрипта и бинарный формат .MTL.

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::parser::parse_line;
use encoding_rs::WINDOWS_1251;

pub const RECORD_SIZE: usize = 89;
pub const NAME_FIELD_SIZE: usize = 50;
pub const NUMERIC_FIELD_SIZE: usize = 10;
pub const NAME_ENCODING: &str = "windows-1251";
pub const NUMERIC_ENCODING: &str = "ascii";
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialEntry {
    pub name: String,
    pub count: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MtlMaterial {
    pub name: String,
    pub thermal_x: f64,
    pub thermal_y: f64,
    pub volume_heat: f64,
    pub rgb_r: u8,
    pub rgb_g: u8,
    pub rgb_b: u8,
    pub special_value: u8,
}

#[derive(Clone, Debug)]
pub struct MtlRecord {
    pub material: MtlMaterial,
    pub raw: Vec<u8>,
}

pub fn normalize_material_name(name: &str) -> String {
    let collapsed: Vec<&str> = name.split_whitespace().collect();
    let s = collapsed.join(" ").to_lowercase();
    // Python использует str.casefold() (агрессивное Unicode-folding:
    // ß→ss, ﬀ→ff и т.д.). Для HEAT3-имён (кириллица + латиница)
    // to_lowercase() покрывает почти все случаи; ß обрабатываем явно.
    s.replace('ß', "ss")
        .replace("ſ", "s") // long s
        .replace("ℌ", "h") // some folds that lowercase may not handle
}

/// Каноническое имя материала из trailing-части строки скрипта HEAT3.
/// Учитывает все форматы:
/// - `Имя! material box` (и вообще `Имя!...`) — имя ДО '!';
/// - `...;Имя`, `...//Имя`, `...#Имя` — имя ПОСЛЕ разделителя.
///   Возвращает нормализованное имя (как ключи в карте материалов).
pub fn material_name_from_trailing(trailing: &str) -> String {
    normalize_material_name(&material_name_text_from_trailing(trailing))
}

fn material_name_text_from_trailing(trailing: &str) -> String {
    let t = trailing.trim();
    let candidate = if let Some(idx) = t.find('!') {
        t[..idx].trim()
    } else if let Some(idx) = t.find(';') {
        t[idx + 1..].trim()
    } else if let Some(idx) = t.find("//") {
        t[idx + 2..].trim()
    } else if let Some(idx) = t.find('#') {
        t[idx + 1..].trim()
    } else {
        t
    };
    strip_material_box_options(candidate)
}

/// Removes HEAT3 material-box option tokens (`%hide`, `%T=value`) from the
/// trailing text so only the material name remains. The original trailing text
/// is preserved untouched in the script line, so serialization/transforms keep
/// the options intact; only name resolution strips them.
fn strip_material_box_options(text: &str) -> String {
    text.split_whitespace()
        .filter(|token| {
            let lower = token.to_ascii_lowercase();
            lower != "%hide" && !lower.starts_with("%t=")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn split_line_ending(text: &str) -> (String, String) {
    crate::text::split_line_ending(text)
}

fn material_name_from_raw_line(raw_line: &str) -> Option<String> {
    let (line_text, _) = split_line_ending(raw_line);
    let line = parse_line(&line_text);
    if line.label.as_deref() == Some("p") {
        if line.segment.is_some() {
            let material_name = material_name_text_from_trailing(&line.trailing);
            if !material_name.is_empty() {
                return Some(material_name);
            }
            return None;
        }
        return None;
    }
    // Official HEAT3 also defines `s x1 y1 z1 dx dy dz material` as a material box.
    // It is not yet parsed by `parse_script` into a segment, so handle its trailing
    // material name directly from the raw text.
    let trimmed = line_text.trim_start();
    if trimmed.starts_with("s ") || trimmed.starts_with("s\t") {
        let after_s = trimmed[1..].trim_start();
        let mut remainder = after_s;
        for _ in 0..6 {
            let (token, after) = crate::parser::take_token(remainder)?;
            // Ensure the token is a finite number; otherwise this is not a valid `s` box.
            if token.parse::<f64>().map(|v| !v.is_finite()).unwrap_or(true) {
                return None;
            }
            remainder = after;
        }
        let material_name = material_name_text_from_trailing(remainder);
        if material_name.is_empty() {
            return None;
        }
        return Some(material_name);
    }
    None
}

pub fn extract_material_entries(script_text: &str) -> Vec<MaterialEntry> {
    let mut materials: HashMap<String, (String, i32)> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for (raw_line, _) in crate::text::split_lines(script_text) {
        let material_name = match material_name_from_raw_line(&raw_line) {
            Some(n) => n,
            None => continue,
        };
        let key = normalize_material_name(&material_name);
        if !materials.contains_key(&key) {
            order.push(key.clone());
        }
        let (original_name, count) = materials
            .get(&key)
            .cloned()
            .unwrap_or_else(|| (material_name.clone(), 0));
        materials.insert(key, (original_name, count + 1));
    }
    order
        .into_iter()
        .map(|key| {
            let (name, count) = &materials[&key];
            MaterialEntry {
                name: name.clone(),
                count: *count,
            }
        })
        .collect()
}

pub fn sort_material_boxes_by_order(script_text: &str, material_order: &[String]) -> String {
    let rank_by_name: HashMap<String, usize> = material_order
        .iter()
        .enumerate()
        .map(|(i, name)| (normalize_material_name(name), i))
        .collect();

    let mut lines: Vec<(String, String)> = crate::text::split_lines(script_text);
    let mut material_slots: Vec<usize> = Vec::new();
    let mut material_lines: Vec<(String, String, usize)> = Vec::new();

    for (index, (line_text, _)) in lines.iter().enumerate() {
        let material_name = match material_name_from_raw_line(line_text) {
            Some(n) => n,
            None => continue,
        };
        material_slots.push(index);
        material_lines.push((line_text.clone(), material_name, index));
    }

    let mut sorted_material_lines = material_lines.clone();
    sorted_material_lines.sort_by(|a, b| {
        let key_a = normalize_material_name(&a.1);
        let key_b = normalize_material_name(&b.1);
        let ra = rank_by_name.get(&key_a);
        let rb = rank_by_name.get(&key_b);
        match (ra, rb) {
            (Some(x), Some(y)) => x.cmp(y).then(a.2.cmp(&b.2)),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.2.cmp(&b.2),
        }
    });

    for (slot_index, (line_text, _, _)) in material_slots.iter().zip(sorted_material_lines.iter()) {
        let (_, brk) = &lines[*slot_index];
        lines[*slot_index] = (line_text.clone(), brk.clone());
    }

    lines
        .iter()
        .map(|(text, brk)| format!("{}{}", text, brk))
        .collect()
}

pub fn is_material_reorder_safe(script: &str) -> bool {
    let lines = crate::parser::parse_script(script);
    let p_segments: Vec<crate::models::Segment> = lines
        .iter()
        .filter(|l| l.label.as_deref() == Some("p"))
        .filter_map(|l| l.segment)
        .collect();
    let e_segments: Vec<crate::models::Segment> = lines
        .iter()
        .filter(|l| l.label.as_deref() == Some("e"))
        .filter_map(|l| l.segment)
        .collect();
    for i in 0..p_segments.len() {
        for j in (i + 1)..p_segments.len() {
            if boxes_overlap(&p_segments[i], &p_segments[j]) {
                return false;
            }
        }
    }
    for p in &p_segments {
        for e in &e_segments {
            if boxes_overlap(p, e) {
                return false;
            }
        }
    }
    true
}

fn boxes_overlap(a: &crate::models::Segment, b: &crate::models::Segment) -> bool {
    let [ax1, ay1, az1, ax2, ay2, az2] = a.as_tuple();
    let [bx1, by1, bz1, bx2, by2, bz2] = b.as_tuple();
    let (ax_min, ax_max) = (ax1.min(ax2), ax1.max(ax2));
    let (ay_min, ay_max) = (ay1.min(ay2), ay1.max(ay2));
    let (az_min, az_max) = (az1.min(az2), az1.max(az2));
    let (bx_min, bx_max) = (bx1.min(bx2), bx1.max(bx2));
    let (by_min, by_max) = (by1.min(by2), by1.max(by2));
    let (bz_min, bz_max) = (bz1.min(bz2), bz1.max(bz2));
    const EPS: f64 = 1e-9;
    (ax_max.min(bx_max) - ax_min.max(bx_min) > EPS)
        && (ay_max.min(by_max) - ay_min.max(by_min) > EPS)
        && (az_max.min(bz_max) - az_min.max(bz_min) > EPS)
}

fn decode_text(raw: &[u8], encoding: &str) -> String {
    let decoded = if encoding == NAME_ENCODING {
        let (text, _) = WINDOWS_1251.decode_without_bom_handling(raw);
        text.to_string()
    } else {
        // Python: raw.decode("ascii", errors="replace") — не-ASCII -> '?'
        raw.iter()
            .map(|&b| if b < 128 { b as char } else { '?' })
            .collect::<String>()
    };
    decoded.trim_end_matches('\u{0}').trim().to_string()
}

fn to_float(raw: &[u8], label: &str) -> Result<f64, String> {
    let text = decode_text(raw, NUMERIC_ENCODING);
    if text.is_empty() {
        return Err(format!("{}: empty numeric field", label));
    }
    let parsed = text
        .replace(',', ".")
        .parse::<f64>()
        .map_err(|_| format!("{}: invalid numeric value {:?}", label, text))?;
    if !parsed.is_finite() {
        return Err(format!("{}: non-finite numeric value {:?}", label, text));
    }
    Ok(parsed)
}

fn read_padded_field(
    data: &[u8],
    offset: usize,
    field_size: usize,
    label: &str,
) -> Result<(Vec<u8>, usize), String> {
    if offset >= data.len() {
        return Err(format!(
            "{}: missing length byte at offset {}",
            label, offset
        ));
    }
    let declared_len = data[offset] as usize;
    let offset = offset + 1;
    if declared_len > field_size {
        return Err(format!(
            "{}: declared length {} exceeds slot size {}",
            label, declared_len, field_size
        ));
    }
    let end = offset + field_size;
    if end > data.len() {
        return Err(format!("{}: truncated field at offset {}", label, offset));
    }
    Ok((data[offset..offset + declared_len].to_vec(), end))
}

fn parse_mtl_record(data: &[u8], start: usize) -> Result<MtlMaterial, String> {
    let record = &data[start..start + RECORD_SIZE];
    if record.len() != RECORD_SIZE {
        return Err(format!("record at offset {} is truncated", start));
    }
    let offset = 0;
    let (name_raw, offset) = read_padded_field(record, offset, NAME_FIELD_SIZE, "name")?;
    let (tx_raw, offset) = read_padded_field(record, offset, NUMERIC_FIELD_SIZE, "thermal_x")?;
    let (ty_raw, offset) = read_padded_field(record, offset, NUMERIC_FIELD_SIZE, "thermal_y")?;
    let (vh_raw, offset) = read_padded_field(record, offset, NUMERIC_FIELD_SIZE, "volume_heat")?;
    let tail = &record[offset..offset + 5];
    if tail.len() != 5 {
        return Err(format!("record at offset {} has truncated tail", start));
    }
    let (rgb_r, rgb_g, rgb_b, _reserved, special_value) =
        (tail[0], tail[1], tail[2], tail[3], tail[4]);

    let name = decode_text(&name_raw, NAME_ENCODING);
    if name.trim().is_empty() {
        // An empty/whitespace name is an integrity error rather than a record
        // to silently drop: strict callers must fail, permissive callers skip.
        return Err(format!(
            "record at offset {start} has an empty material name"
        ));
    }

    Ok(MtlMaterial {
        name,
        thermal_x: to_float(&tx_raw, "thermal_x")?,
        thermal_y: to_float(&ty_raw, "thermal_y")?,
        volume_heat: to_float(&vh_raw, "volume_heat")?,
        rgb_r,
        rgb_g,
        rgb_b,
        special_value,
    })
}

pub fn parse_mtl_records(path: &Path, strict: bool) -> Result<Vec<MtlRecord>, String> {
    let data = std::fs::read(path).map_err(|e| format!("cannot read file: {}", e))?;
    let (record_count, remainder) = (data.len() / RECORD_SIZE, data.len() % RECORD_SIZE);
    if remainder != 0 && strict {
        return Err(format!(
            "file size {} is not a multiple of record size {}",
            data.len(),
            RECORD_SIZE
        ));
    }
    let mut records: Vec<MtlRecord> = Vec::new();
    for index in 0..record_count {
        let start = index * RECORD_SIZE;
        let raw = data[start..start + RECORD_SIZE].to_vec();
        match parse_mtl_record(&data, start) {
            Ok(material) => records.push(MtlRecord { material, raw }),
            Err(_) if !strict => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(records)
}

pub fn parse_mtl_file(path: &Path, strict: bool) -> Result<Vec<MtlMaterial>, String> {
    // No post-filter: empty-name records are already rejected by
    // `parse_mtl_record` so strict loads fail loudly instead of dropping a
    // record after a nominally successful parse.
    Ok(parse_mtl_records(path, strict)?
        .into_iter()
        .map(|r| r.material)
        .collect())
}

fn encode_padded(
    text: &str,
    field_size: usize,
    encoding: &str,
    label: &str,
) -> Result<Vec<u8>, String> {
    let encoded: Vec<u8> = if encoding == NAME_ENCODING {
        let (cow, _, had_errors) = WINDOWS_1251.encode(text);
        if had_errors {
            return Err(format!(
                "{} contains characters outside Windows-1251",
                label
            ));
        }
        cow.into_owned()
    } else {
        text.bytes()
            .map(|b| if b < 128 { b } else { b'?' })
            .collect()
    };
    if encoded.len() > field_size {
        return Err(format!(
            "{} too long: {} bytes, max {}",
            label,
            encoded.len(),
            field_size
        ));
    }
    let mut out = encoded;
    out.extend(std::iter::repeat_n(0u8, field_size - out.len()));
    Ok(out)
}

fn format_float(value: f64, label: &str) -> Result<Vec<u8>, String> {
    if !value.is_finite() {
        return Err(format!("{} must be finite", label));
    }
    let mut candidates: Vec<String> = Vec::new();
    if value.fract() == 0.0 && value.abs() < 1e16 {
        candidates.push(format!("{}", value as i64)); // Python: str(int(v))
    }
    // Python: repr(float(v)) — shortest round-trip (~17 sig digits)
    candidates.push(format!("{:.17}", value));
    // Python: {:.10g} и {:.6g} — через format_g из text.rs
    candidates.push(crate::text::format_g(value, 10));
    candidates.push(crate::text::format_g(value, 6));
    candidates.push(format!("{:.4e}", value)); // Python: {:.4e}
    candidates.push(format!("{:.3e}", value)); // Python: {:.3e}
    for c in candidates {
        if !c.is_empty() && c.len() <= NUMERIC_FIELD_SIZE {
            return Ok(c.into_bytes());
        }
    }
    Err(format!(
        "{} does not fit in {} bytes",
        value, NUMERIC_FIELD_SIZE
    ))
}

pub fn pack_mtl_record(material: &MtlMaterial) -> Result<Vec<u8>, String> {
    let name = material.name.trim_matches('\u{0}').trim().to_string();
    let name_slot = encode_padded(&name, NAME_FIELD_SIZE, NAME_ENCODING, "name")?;
    let tx = format_float(material.thermal_x, "thermal_x")?;
    let ty = format_float(material.thermal_y, "thermal_y")?;
    let vh = format_float(material.volume_heat, "volume_heat")?;

    for (label, value) in [
        ("rgb_r", material.rgb_r as i32),
        ("rgb_g", material.rgb_g as i32),
        ("rgb_b", material.rgb_b as i32),
        ("special_value", material.special_value as i32),
    ] {
        if !(0..=255).contains(&value) {
            return Err(format!("{} out of uint8 range [0..255]: {}", label, value));
        }
    }

    let name_len = name_slot
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(name_slot.len());

    let mut record: Vec<u8> = Vec::new();
    record.push(name_len as u8);
    record.extend_from_slice(&name_slot);
    record.push(tx.len() as u8);
    record.extend_from_slice(&pad10(&tx));
    record.push(ty.len() as u8);
    record.extend_from_slice(&pad10(&ty));
    record.push(vh.len() as u8);
    record.extend_from_slice(&pad10(&vh));
    record.extend_from_slice(&[
        material.rgb_r,
        material.rgb_g,
        material.rgb_b,
        0,
        material.special_value,
    ]);

    if record.len() != RECORD_SIZE {
        return Err(format!("record size {} != {}", record.len(), RECORD_SIZE));
    }
    Ok(record)
}

fn pad10(bytes: &[u8]) -> Vec<u8> {
    let mut v = bytes.to_vec();
    v.extend(std::iter::repeat_n(0u8, NUMERIC_FIELD_SIZE - v.len()));
    v
}

pub fn write_mtl_file(path: &Path, materials: &[MtlMaterial]) -> Result<(), String> {
    let mut data: Vec<u8> = Vec::new();
    for material in materials {
        data.extend_from_slice(&pack_mtl_record(material)?);
    }
    atomic_write_bytes(path, &data)
}

/// Nanosecond timestamp plus a process-local counter: parallel writers in the
/// same nanosecond must still get distinct temp file names on Windows.
fn atomic_temp_suffix() -> String {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let counter = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{}-{}-{}", std::process::id(), nanos, counter)
}

pub fn atomic_write_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    atomic_write_bytes_with_replace(path, bytes, atomic_replace)
}

fn atomic_write_bytes_with_replace(
    path: &Path,
    bytes: &[u8],
    replace: impl FnOnce(&Path, &Path) -> Result<(), String>,
) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "cannot write file: target path has no parent".to_owned())?;
    let unique = atomic_temp_suffix();
    let temporary = parent.join(format!(".mtl-{unique}.tmp"));

    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| format!("cannot write file: {}", e))?;
        file.write_all(bytes)
            .map_err(|e| format!("cannot write file: {}", e))?;
        file.sync_all()
            .map_err(|e| format!("cannot write file: {}", e))?;
        replace(&temporary, path)
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(windows)]
fn atomic_replace(source: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let ok = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok == 0 {
        return Err(format!(
            "cannot write file: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn atomic_replace(_source: &Path, _destination: &Path) -> Result<(), String> {
    Err("cannot write file: atomic replace is unsupported on this platform".to_owned())
}

pub fn materials_by_normalized_name(materials: &[MtlMaterial]) -> HashMap<String, MtlMaterial> {
    materials
        .iter()
        .map(|m| (normalize_material_name(&m.name), m.clone()))
        .collect()
}

/// Compares all material properties that affect conductivity sorting, report
/// values and preview colors, ignoring the display-name spelling.
fn same_material_data(a: &MtlMaterial, b: &MtlMaterial) -> bool {
    a.thermal_x == b.thermal_x
        && a.thermal_y == b.thermal_y
        && a.volume_heat == b.volume_heat
        && a.rgb_r == b.rgb_r
        && a.rgb_g == b.rgb_g
        && a.rgb_b == b.rgb_b
        && a.special_value == b.special_value
}

/// Builds the normalized-name map while rejecting ambiguous duplicates.
///
/// Two records that normalize to the same name are only accepted when every
/// numeric/color property is identical (the first spelling wins for display).
/// Conflicting records would otherwise be silently resolved by file order
/// (last record wins).
pub fn materials_by_normalized_name_checked(
    materials: &[MtlMaterial],
) -> Result<HashMap<String, MtlMaterial>, String> {
    let mut map: HashMap<String, MtlMaterial> = HashMap::new();
    let mut conflicts: Vec<String> = Vec::new();
    for material in materials {
        let key = normalize_material_name(&material.name);
        match map.get(&key) {
            Some(existing) if !same_material_data(existing, material) => {
                if !conflicts.contains(&material.name) {
                    conflicts.push(material.name.clone());
                }
            }
            Some(_) => {}
            None => {
                map.insert(key, material.clone());
            }
        }
    }
    if conflicts.is_empty() {
        Ok(map)
    } else {
        Err(format!(
            "MTL содержит конфликтующие дубликаты материалов: {}.",
            conflicts.join(", ")
        ))
    }
}

pub fn sort_material_names_by_conductivity(
    material_names: &[String],
    materials: &HashMap<String, MtlMaterial>,
) -> Vec<String> {
    let mut indexed: Vec<(usize, &String)> = material_names.iter().enumerate().collect();
    indexed.sort_by(|a, b| {
        let ma = materials.get(&normalize_material_name(a.1));
        let mb = materials.get(&normalize_material_name(b.1));
        match (ma, mb) {
            (Some(x), Some(y)) => x
                .thermal_x
                .partial_cmp(&y.thermal_x)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0)),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.0.cmp(&b.0),
        }
    });
    indexed.into_iter().map(|(_, n)| n.clone()).collect()
}

#[cfg(test)]
mod atomic_write_tests {
    use super::*;

    #[test]
    fn failed_atomic_replace_preserves_existing_destination_and_cleans_temp_file() {
        let directory = std::env::temp_dir().join(format!(
            "heat3-atomic-write-failure-{}",
            atomic_temp_suffix()
        ));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("export.stp");
        fs::write(&path, b"previous valid export").unwrap();

        let error =
            atomic_write_bytes_with_replace(&path, b"new export", |temporary, destination| {
                assert!(
                    temporary.exists(),
                    "complete replacement file is staged first"
                );
                assert_eq!(fs::read(temporary).unwrap(), b"new export");
                assert_eq!(destination, path);
                Err("injected atomic replace failure".to_owned())
            })
            .unwrap_err();

        assert_eq!(error, "injected atomic replace failure");
        assert_eq!(fs::read(&path).unwrap(), b"previous valid export");
        let remaining = fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(remaining, vec![path.file_name().unwrap()]);
        fs::remove_dir_all(directory).unwrap();
    }
}
