//! Парсер и сериализатор скриптов HEAT3.
//!
//! Логика парсинга и форматирования построчно повторяет поведение
//! оригинального Povorotnik.py v1.4.

use std::sync::OnceLock;

use crate::config::VALID_LABELS;
pub use crate::models::{ScriptLine, Segment};

fn starts_with_any(line: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|p| line.starts_with(*p))
}

fn line_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"^(p|b|e)\s+([\-\d.\s]+)(.*)$").unwrap())
}

fn number_token_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"[-+]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][-+]?\d+)?").unwrap())
}

fn label_sep_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"^(p|b|e)(\s+)(.*)$").unwrap())
}

pub fn parse_line(line: &str) -> ScriptLine {
    if line.is_empty() || !starts_with_any(line, VALID_LABELS) {
        return ScriptLine::non_script(line);
    }

    let first_is_space = match line.chars().nth(1) {
        None => true,
        Some(c) => !c.is_whitespace(),
    };
    if line.len() == 1 || first_is_space {
        return ScriptLine::non_script(line);
    }

    let stripped = line.trim();
    let caps = match line_re().captures(stripped) {
        None => return ScriptLine::non_script(line),
        Some(c) => c,
    };

    let label = caps.get(1).unwrap().as_str().to_string();
    let coords_raw = caps.get(2).unwrap().as_str();
    let trailing_raw = caps.get(3).unwrap().as_str().trim().to_string();

    let parts: Vec<&str> = coords_raw.split_whitespace().collect();
    if parts.len() < 6 {
        return ScriptLine::non_script(line);
    }

    let mut nums = [0.0f64; 6];
    for (i, p) in parts[..6].iter().enumerate() {
        match p.parse::<f64>() {
            // Отклоняем NaN/inf/-inf: `parse::<f64>()` принимает их, но они не имеют смысла
            // как координаты HEAT3 и привели бы к panic в sort_by/cavity-detection下游.
            Ok(v) if v.is_finite() => nums[i] = v,
            _ => return ScriptLine::non_script(line),
        }
    }

    let mut extra_values: Vec<f64> = Vec::new();
    let mut extra_raw: Vec<String> = Vec::new();
    if parts.len() > 6 {
        for p in &parts[6..] {
            if let Ok(v) = p.parse::<f64>() {
                extra_values.push(v);
                extra_raw.push(p.to_string());
            }
        }
    }

    ScriptLine {
        raw: line.to_string(),
        label: Some(label),
        segment: Some(Segment::new(
            nums[0], nums[1], nums[2], nums[3], nums[4], nums[5],
        )),
        trailing: trailing_raw,
        extra_values,
        extra_raw,
        line_break: None,
    }
}

pub fn parse_script(text: &str) -> Vec<ScriptLine> {
    let mut lines: Vec<ScriptLine> = Vec::new();
    for (content, brk) in crate::text::split_lines(text) {
        let mut line = parse_line(&content);
        line.line_break = Some(brk);
        lines.push(line);
    }
    lines
}

pub fn format_extra_values(values: &[f64]) -> Vec<String> {
    // Python: f"{v:.0f}" — округление до целого (банковское)
    values
        .iter()
        .map(|v| {
            let s = format!("{:.0}", v);
            if s == "-0" {
                "0".to_string()
            } else {
                s
            }
        })
        .collect()
}

pub fn format_segment_values(segment: &Segment) -> Vec<String> {
    segment
        .as_tuple()
        .iter()
        .map(|c| format!("{:.4}", c))
        .collect()
}

pub fn format_segment(segment: &Segment) -> String {
    format_segment_values(segment).join(" ")
}

fn char_substring(s: &str, start: usize, end: usize) -> String {
    s.char_indices()
        .skip(start)
        .take(end - start)
        .map(|(_, c)| c)
        .collect()
}

pub fn serialize_line(line: &ScriptLine) -> String {
    let (segment, label) = match (&line.segment, &line.label) {
        (None, _) | (_, None) => return line.raw.clone(),
        (Some(s), Some(l)) => (s, l),
    };

    let mut formatted_numbers = format_segment_values(segment);
    if !line.extra_values.is_empty() {
        formatted_numbers.extend(format_extra_values(&line.extra_values));
    }

    if let Some(preserved) = serialize_with_original_separators(line, &formatted_numbers) {
        return preserved;
    }

    let mut out = format!("{} {}", label, format_segment(segment));
    if !line.extra_values.is_empty() {
        out.push(' ');
        if line.extra_raw.len() == line.extra_values.len() {
            out.push_str(&line.extra_raw.join(" "));
        } else {
            out.push_str(&format_extra_values(&line.extra_values).join(" "));
        }
    }
    if !line.trailing.is_empty() {
        out.push(' ');
        out.push_str(&line.trailing);
    }
    out
}

fn serialize_with_original_separators(
    line: &ScriptLine,
    formatted_numbers: &[String],
) -> Option<String> {
    let caps = label_sep_re().captures(&line.raw)?;
    let label = caps.get(1).unwrap().as_str();
    let first_separator = caps.get(2).unwrap().as_str();
    let rest = caps.get(3).unwrap().as_str();

    let mut parts = String::new();
    parts.push_str(label);
    parts.push_str(first_separator);

    let mut position = 0usize;
    for formatted in formatted_numbers {
        let m = number_token_re().find_at(rest, position)?;
        parts.push_str(&rest[position..m.start()]);
        parts.push_str(formatted);
        position = m.end();
    }
    parts.push_str(&format_original_tail(&rest[position..], &line.trailing));
    Some(parts)
}

fn format_original_tail(original_tail: &str, current_trailing: &str) -> String {
    if current_trailing.is_empty() {
        return original_tail.to_string();
    }
    let original_trailing = original_tail.trim();
    if original_trailing.is_empty() || original_trailing == current_trailing {
        return original_tail.to_string();
    }

    let leading_len = original_tail.len() - original_tail.trim_start().len();
    let trailing_len = original_tail.len() - original_tail.trim_end().len();
    let leading = char_substring(original_tail, 0, leading_len);
    let trailing = if trailing_len > 0 {
        char_substring(
            original_tail,
            original_tail.chars().count() - trailing_len,
            original_tail.chars().count(),
        )
    } else {
        String::new()
    };
    format!("{}{}{}", leading, current_trailing, trailing)
}

pub fn serialize_script(lines: &[ScriptLine], line_break: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        parts.push(serialize_line(line));
        if let Some(brk) = &line.line_break {
            if !brk.is_empty() {
                parts.push(brk.clone());
            }
        } else if index < lines.len() - 1 {
            parts.push(line_break.to_string());
        }
    }
    parts.concat()
}
