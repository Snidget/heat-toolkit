//! Разбор лога воздушных прослоек HEAT3 и генерация/обновление материалов .MTL.

use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

use crate::material_sort::{
    atomic_write_bytes, normalize_material_name, pack_mtl_record, parse_mtl_records, MtlMaterial,
};

pub const DEFAULT_AIR_CAVITY_NAME_MASK: &str = "Прослойка {n:03d}";
pub const ALLOWED_NAME_FIELDS: &[&str] = &["n", "b", "d", "area", "lambda", "lam"];
const MAX_FORMAT_WIDTH_OR_PRECISION: usize = 32;

pub fn simple_name_tokens() -> &'static HashMap<&'static str, &'static str> {
    static MAP: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut m = HashMap::new();
        m.insert("\u{43d}\u{43e}\u{43c}\u{435}\u{440}", "n");
        m.insert("номер", "n");
        m.insert("n", "n");
        m.insert("\u{448}\u{438}\u{440}\u{438}\u{43d}\u{430}", "b");
        m.insert("ширина", "b");
        m.insert("b", "b");
        m.insert("\u{433}\u{43b}\u{443}\u{431}\u{438}\u{43d}\u{430}", "d");
        m.insert("глубина", "d");
        m.insert("d", "d");
        m.insert("\u{43f}\u{43b}\u{43e}\u{449}\u{430}\u{434}\u{44c}", "area");
        m.insert("площадь", "area");
        m.insert("area", "area");
        m.insert("\u{43b}\u{44f}\u{43c}\u{431}\u{434}\u{430}", "lambda");
        m.insert("лямбда", "lambda");
        m.insert("lambda", "lambda");
        m.insert("lam", "lambda");
        m
    })
}

fn simple_token_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"\[([^\[\]]+)\]").unwrap())
}

#[derive(Clone, Debug, PartialEq)]
pub struct AirCavity {
    pub number: i32,
    pub b_mm: f64,
    pub d_mm: f64,
    pub area_mm2: f64,
    pub lambda_value: f64,
    pub iteration: i32,
}

#[derive(Clone, Debug)]
pub struct AirCavityMaterialSpec {
    pub cavity: AirCavity,
    pub material: MtlMaterial,
}

#[derive(Clone, Debug)]
pub struct MtlUpsertResult {
    pub path: String,
    pub updated_names: Vec<String>,
    pub added_names: Vec<String>,
}

impl MtlUpsertResult {
    pub fn updated_count(&self) -> usize {
        self.updated_names.len()
    }
    pub fn added_count(&self) -> usize {
        self.added_names.len()
    }
}

#[derive(Clone, Debug)]
struct CavityDimensions {
    number: i32,
    b_mm: f64,
    d_mm: f64,
    area_mm2: f64,
}

#[derive(Clone, Debug)]
struct CavityConductivity {
    number: i32,
    lambda_value: f64,
    iteration: i32,
}

fn iter_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"(?i)\bIter\s*:\s*(\d+)").unwrap())
}

fn normalize_line(line: &str) -> String {
    line.to_lowercase()
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ")
}

fn is_dimensions_header(line: &str) -> bool {
    let n = normalize_line(line);
    n.starts_with("cavity") && n.contains("b [mm]") && n.contains("d [mm]") && n.contains("area")
}

fn is_lambda_header(line: &str) -> bool {
    let n = normalize_line(line);
    n.starts_with("cavity") && n.contains("lambda") && n.contains("iter:")
}

fn parse_positive_float(value: &str, label: &str) -> Result<f64, String> {
    let parsed = value
        .replace(',', ".")
        .parse::<f64>()
        .map_err(|_| format!("{}: некорректное число {:?}.", label, value))?;
    if !parsed.is_finite() || parsed <= 0.0 {
        return Err(format!("{}: значение должно быть больше 0.", label));
    }
    Ok(parsed)
}

fn parse_dimensions_row(line: &str) -> Option<CavityDimensions> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 4 || !parts[0].chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let number: i32 = parts[0].parse().ok()?;
    let b_mm = parse_positive_float(parts[1], &format!("Cavity {} b", number)).ok()?;
    let d_mm = parse_positive_float(parts[2], &format!("Cavity {} d", number)).ok()?;
    let area_mm2 = parse_positive_float(parts[3], &format!("Cavity {} area", number)).ok()?;
    Some(CavityDimensions {
        number,
        b_mm,
        d_mm,
        area_mm2,
    })
}

fn parse_lambda_row(line: &str, iteration: i32) -> Option<CavityConductivity> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 6 || !parts[0].chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let number: i32 = parts[0].parse().ok()?;
    let lambda_value = parse_positive_float(parts[5], &format!("Cavity {} lambda", number)).ok()?;
    Some(CavityConductivity {
        number,
        lambda_value,
        iteration,
    })
}

fn read_dimensions_table(lines: &[String], start: usize) -> (Vec<CavityDimensions>, usize) {
    let mut rows = Vec::new();
    let mut index = start;
    while index < lines.len() {
        match parse_dimensions_row(&lines[index]) {
            None => {
                if !rows.is_empty() {
                    break;
                }
                index += 1;
            }
            Some(r) => {
                rows.push(r);
                index += 1;
            }
        }
    }
    (rows, index)
}

fn read_lambda_table(
    lines: &[String],
    start: usize,
    iteration: i32,
) -> (Vec<CavityConductivity>, usize) {
    let mut rows = Vec::new();
    let mut index = start;
    while index < lines.len() {
        match parse_lambda_row(&lines[index], iteration) {
            None => {
                if !rows.is_empty() {
                    break;
                }
                index += 1;
            }
            Some(r) => {
                rows.push(r);
                index += 1;
            }
        }
    }
    (rows, index)
}

pub fn parse_air_cavities_info_log(text: &str) -> Result<Vec<AirCavity>, String> {
    let lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();
    let mut latest_dimensions: HashMap<i32, CavityDimensions> = HashMap::new();
    let mut lambda_tables: Vec<(i32, Vec<CavityConductivity>, HashMap<i32, CavityDimensions>)> =
        Vec::new();

    let mut index = 0;
    while index < lines.len() {
        let line = &lines[index];
        if is_dimensions_header(line) {
            let (rows, next) = read_dimensions_table(&lines, index + 1);
            if !rows.is_empty() {
                latest_dimensions = rows.into_iter().map(|r| (r.number, r)).collect();
            }
            index = next;
            continue;
        }
        if is_lambda_header(line) {
            let iteration = iter_re()
                .captures(line)
                .and_then(|c| c.get(1))
                .and_then(|m| m.as_str().parse::<i32>().ok())
                .unwrap_or(0);
            let (rows, next) = read_lambda_table(&lines, index + 1, iteration);
            if !rows.is_empty() {
                lambda_tables.push((iteration, rows, latest_dimensions.clone()));
            }
            index = next;
            continue;
        }
        index += 1;
    }

    if latest_dimensions.is_empty() {
        return Err("В логе не найдена таблица размеров Cavity b d area.".to_string());
    }
    if lambda_tables.is_empty() {
        return Err("В логе не найдена таблица Cavity с lambda.".to_string());
    }

    let (_iteration, conductivities, dimensions) = &lambda_tables[lambda_tables.len() - 1];
    if dimensions.is_empty() {
        return Err("Для последней таблицы lambda не найдена таблица размеров.".to_string());
    }

    let dimension_numbers: std::collections::HashSet<i32> = dimensions.keys().copied().collect();
    let conductivity_numbers: std::collections::HashSet<i32> =
        conductivities.iter().map(|r| r.number).collect();
    if dimension_numbers != conductivity_numbers {
        let mut missing_dimensions: Vec<i32> = conductivity_numbers
            .difference(&dimension_numbers)
            .copied()
            .collect();
        let mut missing_lambda: Vec<i32> = dimension_numbers
            .difference(&conductivity_numbers)
            .copied()
            .collect();
        missing_dimensions.sort();
        missing_lambda.sort();
        let mut details: Vec<String> = Vec::new();
        if !missing_dimensions.is_empty() {
            details.push(format!(
                "\u{43d}\u{435}\u{442} \u{440}\u{430}\u{437}\u{43c}\u{435}\u{440}\u{43e}\u{432} \u{434}\u{43b}\u{44f} Cavity {:?}",
                missing_dimensions
            ));
        }
        if !missing_lambda.is_empty() {
            details.push(format!(
                "\u{43d}\u{435}\u{442} lambda \u{434}\u{43b}\u{44f} Cavity {:?}",
                missing_lambda
            ));
        }
        return Err(details.join("; "));
    }

    let mut cavities: Vec<AirCavity> = Vec::new();
    for row in conductivities {
        let dims = &dimensions[&row.number];
        cavities.push(AirCavity {
            number: row.number,
            b_mm: dims.b_mm,
            d_mm: dims.d_mm,
            area_mm2: dims.area_mm2,
            lambda_value: row.lambda_value,
            iteration: row.iteration,
        });
    }
    Ok(cavities)
}

fn format_number_default(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        let mut s = format!("{:.6}", value);
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
        if s == "-0" {
            s = "0".to_string();
        }
        s
    }
}

fn format_mask_value(value: f64, spec: &str) -> String {
    if spec.is_empty() {
        return format_number_default(value);
    }
    let spec = spec.trim();
    // Определяем тип формата по последнему символу
    let fmt_type = spec.chars().last().unwrap_or_default();
    match fmt_type {
        'd' | 'i' | 'u' => {
            let width_part: String = spec.chars().take_while(|c| c.is_ascii_digit()).collect();
            let width: usize = width_part.parse().unwrap_or(0);
            let iv = value.round() as i64;
            if spec.starts_with('0') && width > 0 {
                format!("{:0width$}", iv, width = width)
            } else {
                iv.to_string()
            }
        }
        'f' | 'F' => {
            // Синтаксис: .<точность>f или <ширина>.<точность>f
            // Извлекаем точность из spec (всё между точкой и f)
            let prec: usize = spec
                .split('.')
                .nth(1)
                .and_then(|s| s.trim_end_matches(fmt_type).parse().ok())
                .unwrap_or(6);
            format!("{:.*}", prec, value)
        }
        'e' | 'E' => {
            let prec: usize = spec
                .split('.')
                .nth(1)
                .and_then(|s| s.trim_end_matches(fmt_type).parse().ok())
                .unwrap_or(6);
            let s = format!("{:.*e}", prec, value);
            if fmt_type == 'E' {
                s.replace('e', "E")
            } else {
                s
            }
        }
        'g' | 'G' => {
            let p: usize = spec
                .split('.')
                .nth(1)
                .and_then(|s| s.trim_end_matches(fmt_type).parse().ok())
                .unwrap_or(6);
            let mut s = crate::text::format_g(value, p);
            if fmt_type == 'G' {
                // Python's {:.6G} uses uppercase: Eg -> EG
                s = s.to_uppercase();
            }
            s
        }
        // Любой другой spec (в т.ч. с шириной/выравниванием без явного типа)
        // — используем format_number_default
        _ => format_number_default(value),
    }
}

fn simple_template_to_format_mask(name_mask: &str) -> Result<String, String> {
    let result = simple_token_re().replace_all(name_mask, |caps: &regex::Captures| {
        let token = caps.get(1).unwrap().as_str();
        let normalized: String = token
            .to_lowercase()
            .split_whitespace()
            .collect::<Vec<&str>>()
            .join(" ");
        match simple_name_tokens().get(normalized.as_str()) {
            Some(field) => format!("{{{}}}", field),
            None => format!("__UNKNOWN_{}__", token),
        }
    });
    let s = result.into_owned();
    if s.contains("__UNKNOWN_") {
        let allowed = ALLOWED_NAME_FIELDS
            .iter()
            .map(|f| format!("[{}]", f))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "\u{41d}\u{435}\u{438}\u{437}\u{432}\u{435}\u{441}\u{442}\u{43d}\u{44b}\u{439} \u{43c}\u{430}\u{440}\u{43a}\u{435}\u{440} \u{438}\u{43c}\u{435}\u{43d}\u{438} \u{432} \u{43c}\u{430}\u{441}\u{43a}\u{435}. \u{414}\u{43e}\u{441}\u{442}\u{443}\u{43f}\u{43d}\u{43e}: {}.",
            allowed
        ));
    }
    Ok(s)
}

#[derive(Debug, PartialEq)]
enum NameMaskPart {
    Literal(String),
    Field { name: String, spec: String },
}

fn parse_bounded_format_number(value: &str, label: &str) -> Result<usize, String> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| format!("Некорректный {label} в спецификаторе формата маски имени."))?;
    if parsed > MAX_FORMAT_WIDTH_OR_PRECISION {
        return Err(format!(
            "{label} в спецификаторе формата маски имени не должен превышать {MAX_FORMAT_WIDTH_OR_PRECISION}."
        ));
    }
    Ok(parsed)
}

fn validate_format_spec(spec: &str) -> Result<(), String> {
    if spec.is_empty() {
        return Ok(());
    }
    let Some(format_type) = spec.chars().last() else {
        return Ok(());
    };
    let body = &spec[..spec.len() - format_type.len_utf8()];
    match format_type {
        'd' | 'i' | 'u' => {
            if body.is_empty() {
                return Ok(());
            }
            if !body.chars().all(|character| character.is_ascii_digit()) {
                return Err(
                    "Поддерживаются только целочисленная ширина и d/i/u в маске имени.".to_string(),
                );
            }
            parse_bounded_format_number(body, "ширина")?;
        }
        'f' | 'F' | 'e' | 'E' | 'g' | 'G' => {
            if body.is_empty() {
                return Ok(());
            }
            let Some(precision) = body.strip_prefix('.') else {
                return Err(
                    "Для f/e/g в маске имени поддерживается только точность вида .N.".to_string(),
                );
            };
            if precision.is_empty()
                || !precision
                    .chars()
                    .all(|character| character.is_ascii_digit())
            {
                return Err(
                    "Некорректная точность в спецификаторе формата маски имени.".to_string()
                );
            }
            let precision = parse_bounded_format_number(precision, "точность")?;
            if matches!(format_type, 'g' | 'G') && precision == 0 {
                return Err("Точность g/G в маске имени должна быть больше 0.".to_string());
            }
        }
        _ => {
            return Err(
                "Поддерживаются только спецификаторы d/i/u/f/e/g в маске имени.".to_string(),
            )
        }
    }
    Ok(())
}

fn parse_name_mask(format_mask: &str) -> Result<Vec<NameMaskPart>, String> {
    let mut parts = Vec::new();
    let mut literal = String::new();
    let mut chars = format_mask.chars().peekable();

    while let Some(character) = chars.next() {
        match character {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                literal.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                literal.push('}');
            }
            '{' => {
                if !literal.is_empty() {
                    parts.push(NameMaskPart::Literal(std::mem::take(&mut literal)));
                }

                let mut inner = String::new();
                let mut closed = false;
                for field_character in chars.by_ref() {
                    match field_character {
                        '}' => {
                            closed = true;
                            break;
                        }
                        '{' => {
                            return Err(
                                "Вложенные фигурные скобки в маске не поддерживаются.".to_string()
                            );
                        }
                        _ => inner.push(field_character),
                    }
                }
                if !closed {
                    return Err("Незакрытое поле в маске имени.".to_string());
                }

                let (name, spec) = inner
                    .split_once(':')
                    .map(|(name, spec)| (name.trim(), spec.trim()))
                    .unwrap_or((inner.trim(), ""));
                if name.is_empty() {
                    return Err("Пустые поля {} в маске имени не поддерживаются.".to_string());
                }
                if !ALLOWED_NAME_FIELDS.contains(&name) {
                    let allowed = ALLOWED_NAME_FIELDS
                        .iter()
                        .map(|field| format!("{{{field}}}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    return Err(format!(
                        "\u{41d}\u{435}\u{438}\u{437}\u{432}\u{435}\u{441}\u{442}\u{43d}\u{43e}\u{435} \u{43f}\u{43e}\u{43b}\u{435} \u{43c}\u{430}\u{441}\u{43a}\u{438} {{{name}}}. \u{414}\u{43e}\u{441}\u{442}\u{443}\u{43f}\u{43d}\u{43e}: {allowed}."
                    ));
                }
                validate_format_spec(spec)?;
                parts.push(NameMaskPart::Field {
                    name: name.to_string(),
                    spec: spec.to_string(),
                });
            }
            '}' => return Err("Лишняя закрывающая скобка в маске имени.".to_string()),
            _ => literal.push(character),
        }
    }

    if !literal.is_empty() {
        parts.push(NameMaskPart::Literal(literal));
    }
    Ok(parts)
}

fn validate_name_mask(name_mask: &str) -> Result<(), String> {
    if name_mask.trim().is_empty() {
        return Err("Маска имени не должна быть пустой.".to_string());
    }
    let format_mask = simple_template_to_format_mask(name_mask)?;
    parse_name_mask(&format_mask)?;
    Ok(())
}

fn validate_material_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Итоговое имя материала не должно быть пустым.".to_string());
    }
    match windows_1251_encode(name) {
        Ok(bytes) => {
            if bytes.len() > crate::material_sort::NAME_FIELD_SIZE {
                return Err(format!(
                    "Имя материала {:?} занимает {} байт, максимум {}.",
                    name,
                    bytes.len(),
                    crate::material_sort::NAME_FIELD_SIZE
                ));
            }
        }
        Err(e) => {
            return Err(format!("Имя материала {:?}: {e}.", name));
        }
    }
    Ok(())
}

fn windows_1251_encode(name: &str) -> Result<Vec<u8>, String> {
    use encoding_rs::WINDOWS_1251;
    let (cow, _, had_errors) = WINDOWS_1251.encode(name);
    if had_errors {
        Err("Символы вне Windows-1251".to_string())
    } else {
        Ok(cow.into_owned())
    }
}

pub fn format_air_cavity_name(name_mask: &str, cavity: &AirCavity) -> Result<String, String> {
    let format_mask = simple_template_to_format_mask(name_mask)?;
    let values: HashMap<&str, f64> = [
        ("n", cavity.number as f64),
        ("b", cavity.b_mm),
        ("d", cavity.d_mm),
        ("area", cavity.area_mm2),
        ("lambda", cavity.lambda_value),
        ("lam", cavity.lambda_value),
    ]
    .into_iter()
    .collect();

    let mut out = String::new();
    for part in parse_name_mask(&format_mask)? {
        match part {
            NameMaskPart::Literal(text) => out.push_str(&text),
            NameMaskPart::Field { name, spec } => {
                let value = values
                    .get(name.as_str())
                    .copied()
                    .ok_or_else(|| format!("Неизвестное поле маски {{{name}}}."))?;
                out.push_str(&format_mask_value(value, &spec));
            }
        }
    }
    let name = out.trim().to_string();
    validate_material_name(&name)?;
    Ok(name)
}

pub fn build_air_cavity_materials(
    cavities: &[AirCavity],
    name_mask: &str,
    rgb: (u8, u8, u8),
    special_value: u8,
) -> Result<Vec<AirCavityMaterialSpec>, String> {
    if cavities.is_empty() {
        return Err("Нет воздушных прослоек для создания материалов.".to_string());
    }
    validate_name_mask(name_mask)?;
    let (r, g, b) = validate_rgb(rgb)?;
    let special = validate_uint8(special_value as i32, "Специальное значение")?;

    let mut specs: Vec<AirCavityMaterialSpec> = Vec::new();
    let mut seen_names: HashMap<String, String> = HashMap::new();
    for cavity in cavities {
        let name = format_air_cavity_name(name_mask, cavity)?;
        let key = normalize_material_name(&name);
        if seen_names.contains_key(&key) {
            return Err(format!(
                "\u{41c}\u{430}\u{441}\u{43a}\u{430} \u{438}\u{43c}\u{435}\u{43d}\u{438} \u{441}\u{43e}\u{437}\u{434}\u{430}\u{435}\u{442} \u{434}\u{443}\u{431}\u{43b}\u{438}\u{440}\u{443}\u{44e}\u{449}\u{438}\u{435}\u{441}\u{44f} \u{43c}\u{430}\u{442}\u{435}\u{440}\u{438}\u{430}\u{43b}\u{44b}: {:?} \u{438} {:?}.",
                seen_names[&key], name
            ));
        }
        seen_names.insert(key, name.clone());
        specs.push(AirCavityMaterialSpec {
            cavity: cavity.clone(),
            material: MtlMaterial {
                name,
                thermal_x: cavity.lambda_value,
                thermal_y: cavity.lambda_value,
                volume_heat: 0.0,
                rgb_r: r,
                rgb_g: g,
                rgb_b: b,
                special_value: special,
            },
        });
    }
    Ok(specs)
}

fn validate_rgb(rgb: (u8, u8, u8)) -> Result<(u8, u8, u8), String> {
    Ok(rgb)
}

fn validate_uint8(value: i32, label: &str) -> Result<u8, String> {
    match u8::try_from(value) {
        Ok(v) => Ok(v),
        Err(_) => Err(format!(
            "{}: значение должно быть в диапазоне 0..255.",
            label
        )),
    }
}

/// Prospective effect of an Air Cavities upsert, computed before any write.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MtlUpsertPreflight {
    pub added: Vec<String>,
    pub replaced: Vec<String>,
}

impl MtlUpsertPreflight {
    pub fn has_replacements(&self) -> bool {
        !self.replaced.is_empty()
    }
}

/// Compares generated cavity materials against the existing MTL without
/// mutating it. `replaced` lists existing records that would be overwritten
/// because their normalized name matches a generated material name.
pub fn preflight_air_cavity_upsert(
    path: &Path,
    specs: &[AirCavityMaterialSpec],
) -> Result<MtlUpsertPreflight, String> {
    if specs.is_empty() {
        return Err("Нет материалов для записи.".to_string());
    }
    let mut new_keys: std::collections::HashSet<String> = std::collections::HashSet::new();
    for spec in specs {
        let key = normalize_material_name(&spec.material.name);
        if !new_keys.insert(key) {
            return Err("Список материалов содержит дублирующиеся имена.".to_string());
        }
    }
    let records = parse_mtl_records(path, true)
        .map_err(|e| format!("Не удалось прочитать .MTL файл: {}", e))?;
    let existing_keys: std::collections::HashSet<String> = records
        .iter()
        .map(|record| normalize_material_name(&record.material.name))
        .collect();
    let replaced: Vec<String> = specs
        .iter()
        .filter(|spec| existing_keys.contains(&normalize_material_name(&spec.material.name)))
        .map(|spec| spec.material.name.clone())
        .collect();
    let added: Vec<String> = specs
        .iter()
        .filter(|spec| !existing_keys.contains(&normalize_material_name(&spec.material.name)))
        .map(|spec| spec.material.name.clone())
        .collect();
    Ok(MtlUpsertPreflight { added, replaced })
}

pub fn upsert_air_cavity_materials(
    path: &Path,
    specs: &[AirCavityMaterialSpec],
) -> Result<MtlUpsertResult, String> {
    if specs.is_empty() {
        return Err("Нет материалов для записи.".to_string());
    }
    let new_materials: Vec<MtlMaterial> = specs.iter().map(|s| s.material.clone()).collect();
    let mut new_by_key: HashMap<String, MtlMaterial> = HashMap::new();
    for m in &new_materials {
        new_by_key.insert(normalize_material_name(&m.name), m.clone());
    }
    if new_by_key.len() != new_materials.len() {
        return Err("Список материалов содержит дублирующиеся имена.".to_string());
    }

    let records = parse_mtl_records(path, true)
        .map_err(|e| format!("Не удалось прочитать .MTL файл: {}", e))?;

    let mut updated_keys: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut output: Vec<u8> = Vec::new();
    for record in &records {
        let key = normalize_material_name(&record.material.name);
        match new_by_key.get(&key) {
            None => output.extend_from_slice(&record.raw),
            Some(replacement) => {
                output.extend_from_slice(&pack_mtl_record(replacement)?);
                updated_keys.insert(key);
            }
        }
    }

    for material in &new_materials {
        let key = normalize_material_name(&material.name);
        if !updated_keys.contains(&key) {
            output.extend_from_slice(&pack_mtl_record(material)?);
        }
    }

    atomic_write_bytes(path, &output)
        .map_err(|e| format!("Не удалось записать .MTL файл: {}", e))?;

    let added_names: Vec<String> = new_materials
        .iter()
        .filter(|m| !updated_keys.contains(&normalize_material_name(&m.name)))
        .map(|m| m.name.clone())
        .collect();
    let updated_names: Vec<String> = new_materials
        .iter()
        .filter(|m| updated_keys.contains(&normalize_material_name(&m.name)))
        .map(|m| m.name.clone())
        .collect();

    Ok(MtlUpsertResult {
        path: path.to_string_lossy().to_string(),
        updated_names,
        added_names,
    })
}
