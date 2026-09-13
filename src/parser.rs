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

fn number_token_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"[-+]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][-+]?\d+)?").unwrap())
}

fn label_sep_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"^(p|b|e)(\s+)(.*)$").unwrap())
}

fn take_token(input: &str) -> Option<(&str, &str)> {
    let trimmed = input.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    let end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
    Some((&trimmed[..end], &trimmed[end..]))
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
    let caps = match label_sep_re().captures(stripped) {
        None => return ScriptLine::non_script(line),
        Some(c) => c,
    };

    let label = caps.get(1).unwrap().as_str().to_string();
    let mut remainder = caps.get(3).unwrap().as_str();

    let mut nums = [0.0f64; 6];
    for coordinate in &mut nums {
        let Some((token, after_token)) = take_token(remainder) else {
            return ScriptLine::non_script(line);
        };
        match token.parse::<f64>() {
            // Отклоняем NaN/inf/-inf: `parse::<f64>()` принимает их, но они не имеют смысла
            // как координаты HEAT3 и привели бы к panic в sort_by/cavity-detection下游.
            Ok(v) if v.is_finite() => *coordinate = v,
            _ => return ScriptLine::non_script(line),
        }
        remainder = after_token;
    }

    let mut extra_values: Vec<f64> = Vec::new();
    let mut extra_raw: Vec<String> = Vec::new();
    if label == "b" {
        while let Some((token, after_token)) = take_token(remainder) {
            match token.parse::<f64>() {
                Ok(v) if v.is_finite() => {
                    extra_values.push(v);
                    extra_raw.push(token.to_string());
                    remainder = after_token;
                }
                _ => break,
            }
        }
    }
    let trailing_raw = remainder.trim().to_string();

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
        .map(|c| {
            if *c == 0.0 {
                "0".to_string()
            } else {
                c.to_string()
            }
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_leading_material_name_stays_in_trailing_text() {
        for name in ["0.04 insulation", "12 brick"] {
            let raw = format!("p 0 0 0 1 1 1 {name} ! material box");
            let line = parse_line(&raw);

            assert_eq!(line.trailing, format!("{name} ! material box"));
            assert!(line.extra_values.is_empty());
            assert!(serialize_line(&line).ends_with(&format!("{name} ! material box")));
        }
    }

    #[test]
    fn boundary_condition_fields_remain_transformable_metadata() {
        let line = parse_line("b 0 0 0 1 1 1 2 %enable");

        assert_eq!(line.extra_values, vec![2.0]);
        assert_eq!(line.trailing, "%enable");
        assert!(serialize_line(&line).ends_with("2 %enable"));
    }

    #[test]
    fn empty_box_preserves_numeric_leading_trailing_text() {
        let line = parse_line("e 0 0 0 1 1 1 0.04 cut-out");

        assert_eq!(line.trailing, "0.04 cut-out");
        assert!(line.extra_values.is_empty());
        assert!(serialize_line(&line).ends_with("0.04 cut-out"));
    }

    #[test]
    fn serializer_preserves_sub_millimetre_geometry() {
        let line = parse_line("p -0.00001 0 0 0.00004 0.00002 1 thin ! material box");
        let output = serialize_line(&line);
        let round_tripped = parse_line(&output);

        assert_eq!(
            round_tripped.segment.unwrap().as_tuple(),
            [-0.00001, 0.0, 0.0, 0.00004, 0.00002, 1.0]
        );
        assert!(!output.split_whitespace().any(|token| token == "-0"));
    }

    #[test]
    fn serializer_normalizes_negative_zero_without_rounding_coordinates() {
        let segment = Segment::new(-0.0, 1.0, 1.23456, -0.00001, 2.0, 3.0);

        assert_eq!(
            format_segment_values(&segment),
            vec!["0", "1", "1.23456", "-0.00001", "2", "3"]
        );
    }
}
