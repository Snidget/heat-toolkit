//! Модели данных: координатный отрезок и линия скрипта HEAT3.

/// Отрезок в 3D-пространстве (два конца).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub x1: f64,
    pub y1: f64,
    pub z1: f64,
    pub x2: f64,
    pub y2: f64,
    pub z2: f64,
}

impl Segment {
    pub fn new(x1: f64, y1: f64, z1: f64, x2: f64, y2: f64, z2: f64) -> Self {
        Segment {
            x1,
            y1,
            z1,
            x2,
            y2,
            z2,
        }
    }

    pub fn as_tuple(&self) -> [f64; 6] {
        [self.x1, self.y1, self.z1, self.x2, self.y2, self.z2]
    }
}

/// Одна строка скрипта HEAT3 вида '<label> <coords...> <text>'.
#[derive(Clone, Debug)]
pub struct ScriptLine {
    pub raw: String,
    pub label: Option<String>,
    pub segment: Option<Segment>,
    pub trailing: String,
    pub extra_values: Vec<f64>,
    pub extra_raw: Vec<String>,
    pub line_break: Option<String>,
}

impl ScriptLine {
    pub fn non_script(raw: &str) -> Self {
        ScriptLine {
            raw: raw.to_string(),
            label: None,
            segment: None,
            trailing: String::new(),
            extra_values: Vec::new(),
            extra_raw: Vec::new(),
            line_break: None,
        }
    }
}
