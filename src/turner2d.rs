//! 2D-преобразования прямоугольников (метка `r` или `R`).

use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect2D {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

impl Rect2D {
    pub fn as_tuple(&self) -> [f64; 4] {
        [self.x1, self.y1, self.x2, self.y2]
    }
}

#[derive(Clone, Debug)]
pub struct ScriptLine2D {
    pub raw: String,
    pub rect: Option<Rect2D>,
    pub number_spans: Vec<(usize, usize)>,
    pub line_break: Option<String>,
}

fn number_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"^-?(?:\d+(?:[,.]\d*)?|[,.]\d+)$").unwrap())
}

fn token_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"\S+").unwrap())
}

fn parse_number(text: &str) -> Option<f64> {
    text.replace(',', ".").parse::<f64>().ok()
}

fn normalize_rect(points: [(f64, f64); 2]) -> Rect2D {
    let (ax, ay) = points[0];
    let (bx, by) = points[1];
    Rect2D {
        x1: ax.min(bx),
        y1: ay.min(by),
        x2: ax.max(bx),
        y2: ay.max(by),
    }
}

pub fn parse_2d_line(line: &str) -> ScriptLine2D {
    if line.is_empty() || !matches!(line.as_bytes().first(), Some(b'r' | b'R')) {
        return ScriptLine2D {
            raw: line.to_string(),
            rect: None,
            number_spans: Vec::new(),
            line_break: None,
        };
    }
    let first_is_space = match line.chars().nth(1) {
        None => true,
        Some(c) => !c.is_whitespace(),
    };
    if line.len() == 1 || first_is_space {
        return ScriptLine2D {
            raw: line.to_string(),
            rect: None,
            number_spans: Vec::new(),
            line_break: None,
        };
    }

    let mut tokens: Vec<String> = Vec::new();
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut pos = 1usize;
    while pos < line.len() && tokens.len() < 4 {
        match token_re().find_at(line, pos) {
            None => break,
            Some(m) => {
                let token = m.as_str();
                if !number_re().is_match(token) {
                    return ScriptLine2D {
                        raw: line.to_string(),
                        rect: None,
                        number_spans: Vec::new(),
                        line_break: None,
                    };
                }
                tokens.push(token.to_string());
                spans.push((m.start(), m.end()));
                pos = m.end();
            }
        }
    }

    if tokens.len() < 4 {
        return ScriptLine2D {
            raw: line.to_string(),
            rect: None,
            number_spans: Vec::new(),
            line_break: None,
        };
    }

    let (x1, y1, x2, y2) = match (
        parse_number(&tokens[0]),
        parse_number(&tokens[1]),
        parse_number(&tokens[2]),
        parse_number(&tokens[3]),
    ) {
        (Some(x1), Some(y1), Some(x2), Some(y2)) => (x1, y1, x2, y2),
        _ => {
            return ScriptLine2D {
                raw: line.to_string(),
                rect: None,
                number_spans: Vec::new(),
                line_break: None,
            };
        }
    };

    ScriptLine2D {
        raw: line.to_string(),
        rect: Some(Rect2D { x1, y1, x2, y2 }),
        number_spans: spans,
        line_break: None,
    }
}

pub fn parse_2d_script(text: &str) -> Vec<ScriptLine2D> {
    let mut lines: Vec<ScriptLine2D> = Vec::new();
    for (content, brk) in crate::text::split_lines(text) {
        let mut line = parse_2d_line(&content);
        line.line_break = Some(brk);
        lines.push(line);
    }
    lines
}

fn format_number(value: f64) -> String {
    let value = if value.abs() < 1e-12 { 0.0 } else { value };
    let mut text = format!("{:.6}", value);
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    if text == "-0" {
        text = "0".to_string();
    }
    text
}

fn format_rect_values(rect: &Rect2D) -> Vec<String> {
    rect.as_tuple().iter().map(|v| format_number(*v)).collect()
}

fn apply_replacements(raw: &str, replacements: &[(usize, usize, String)]) -> String {
    let mut sorted = replacements.to_vec();
    sorted.sort_by_key(|r| r.0);
    let mut parts = String::new();
    let mut cursor = 0usize;
    for (start, end, value) in sorted {
        parts.push_str(&raw[cursor..start]);
        parts.push_str(&value);
        cursor = end;
    }
    parts.push_str(&raw[cursor..]);
    parts
}

pub fn serialize_2d_line(line: &ScriptLine2D) -> String {
    let rect = match &line.rect {
        Some(r) if line.number_spans.len() >= 4 => r,
        _ => return line.raw.clone(),
    };

    let values = format_rect_values(rect);
    let replacements: Vec<(usize, usize, String)> = line.number_spans[..4]
        .iter()
        .zip(values.iter())
        .map(|((s, e), v)| (*s, *e, v.clone()))
        .collect();
    apply_replacements(&line.raw, &replacements)
}

pub fn serialize_2d_script(lines: &[ScriptLine2D]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        parts.push(serialize_2d_line(line));
        if let Some(brk) = &line.line_break {
            if !brk.is_empty() {
                parts.push(brk.clone());
            }
        } else if index < lines.len() - 1 {
            parts.push("\r\n".to_string());
        }
    }
    parts.concat()
}

pub fn rotate_clockwise(rect: &Rect2D) -> Rect2D {
    let [x1, y1, x2, y2] = rect.as_tuple();
    normalize_rect([(y1, -x1), (y2, -x2)])
}

pub fn rotate_counterclockwise(rect: &Rect2D) -> Rect2D {
    let [x1, y1, x2, y2] = rect.as_tuple();
    normalize_rect([(-y1, x1), (-y2, x2)])
}

pub fn mirror_x(rect: &Rect2D) -> Rect2D {
    let [x1, y1, x2, y2] = rect.as_tuple();
    normalize_rect([(x1, -y1), (x2, -y2)])
}

pub fn mirror_y(rect: &Rect2D) -> Rect2D {
    let [x1, y1, x2, y2] = rect.as_tuple();
    normalize_rect([(-x1, y1), (-x2, y2)])
}

type Transform2D = fn(&Rect2D) -> Rect2D;

pub const TRANSFORMS_2D: &[(&str, Transform2D)] = &[
    ("rotate_clockwise", rotate_clockwise),
    ("rotate_counterclockwise", rotate_counterclockwise),
    ("mirror_x", mirror_x),
    ("mirror_y", mirror_y),
];

pub fn transform_2d_script(text: &str, transform_name: &str) -> Option<String> {
    let transform = TRANSFORMS_2D
        .iter()
        .find(|(name, _)| *name == transform_name)
        .map(|(_, f)| *f)?;
    let mut lines = parse_2d_script(text);
    for line in lines.iter_mut() {
        if let Some(rect) = &line.rect {
            line.rect = Some(transform(rect));
        }
    }
    Some(serialize_2d_script(&lines))
}
