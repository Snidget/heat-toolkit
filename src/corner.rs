//! Создание угла окна: обрезка и копирование элементов от «петельной» грани.

use std::collections::HashMap;

use crate::models::Segment;
use crate::parser::{parse_script, serialize_script, ScriptLine};

const TOL: f64 = 1e-9;

#[derive(Clone, Debug)]
pub struct ConstantPair {
    pub axis: String,
    pub min_val: f64,
    pub max_val: f64,
}

impl ConstantPair {
    pub fn thickness(&self) -> f64 {
        self.max_val - self.min_val
    }
}

pub fn detect_constant_pair(script_text: &str) -> Option<ConstantPair> {
    let mut pairs = HashMap3::new();

    for line in parse_script(script_text) {
        // Only material (`p`) boxes define the structural section. Boundary
        // (`b`) and empty (`e`) boxes may legitimately have different extents
        // and must not make a valid pseudo-2D material section look invalid.
        if line.label.as_deref() != Some("p") {
            continue;
        }
        let seg = match &line.segment {
            None => continue,
            Some(s) => s,
        };
        for (axis, v1, v2) in [
            ("X", seg.x1, seg.x2),
            ("Y", seg.y1, seg.y2),
            ("Z", seg.z1, seg.z2),
        ] {
            match pairs.get(axis) {
                PairState::Unset => {
                    pairs.set(axis, (v1, v2));
                }
                PairState::Set(val) => {
                    if (v1 - val.0).abs() >= TOL || (v2 - val.1).abs() >= TOL {
                        pairs.mark_none(axis);
                    }
                }
                PairState::None => {}
            }
        }
    }

    for axis in ["X", "Y", "Z"] {
        if let PairState::Set(val) = pairs.get(axis) {
            let (lo, hi) = if val.0 <= val.1 {
                (val.0, val.1)
            } else {
                (val.1, val.0)
            };
            return Some(ConstantPair {
                axis: axis.to_string(),
                min_val: lo,
                max_val: hi,
            });
        }
    }
    None
}

enum PairState {
    Unset,
    Set((f64, f64)),
    None,
}

struct HashMap3 {
    x: PairState,
    y: PairState,
    z: PairState,
}

impl HashMap3 {
    fn new() -> Self {
        HashMap3 {
            x: PairState::Unset,
            y: PairState::Unset,
            z: PairState::Unset,
        }
    }
    fn get(&self, axis: &str) -> &PairState {
        match axis {
            "X" => &self.x,
            "Y" => &self.y,
            "Z" => &self.z,
            _ => &self.x,
        }
    }
    fn set(&mut self, axis: &str, val: (f64, f64)) {
        let state = PairState::Set(val);
        match axis {
            "X" => self.x = state,
            "Y" => self.y = state,
            "Z" => self.z = state,
            _ => {}
        }
    }
    fn mark_none(&mut self, axis: &str) {
        let state = PairState::None;
        match axis {
            "X" => self.x = state,
            "Y" => self.y = state,
            "Z" => self.z = state,
            _ => {}
        }
    }
}

fn same_seg(a: &Segment, b: &Segment) -> bool {
    (a.x1 - b.x1).abs() < TOL
        && (a.x2 - b.x2).abs() < TOL
        && (a.y1 - b.y1).abs() < TOL
        && (a.y2 - b.y2).abs() < TOL
        && (a.z1 - b.z1).abs() < TOL
        && (a.z2 - b.z2).abs() < TOL
}

fn seg_axis(seg: &Segment, axis: &str) -> (f64, f64) {
    match axis {
        "X" => (seg.x1, seg.x2),
        "Y" => (seg.y1, seg.y2),
        "Z" => (seg.z1, seg.z2),
        _ => (0.0, 0.0),
    }
}

fn seg_set(seg: &Segment, axis: &str, v1: f64, v2: f64) -> Segment {
    let mut kw = [seg.x1, seg.y1, seg.z1, seg.x2, seg.y2, seg.z2];
    let (i1, i2) = match axis {
        "X" => (0, 3),
        "Y" => (1, 4),
        "Z" => (2, 5),
        _ => (0, 3),
    };
    kw[i1] = v1;
    kw[i2] = v2;
    Segment::new(kw[0], kw[1], kw[2], kw[3], kw[4], kw[5])
}

fn line_clone(line: &ScriptLine, seg: Segment) -> ScriptLine {
    ScriptLine {
        raw: line.raw.clone(),
        label: line.label.clone(),
        segment: Some(seg),
        trailing: line.trailing.clone(),
        extra_values: line.extra_values.clone(),
        extra_raw: line.extra_raw.clone(),
        line_break: line.line_break.clone(),
    }
}

struct CornerDir {
    hinge_side_max: bool,
    edge_axis: String,
    extend_axis: String,
}

fn direction_to_params(direction: &str, free_axes: &[String]) -> Option<CornerDir> {
    let long_ax = free_axes[0].clone();
    let short_ax = free_axes[1].clone();
    let table: HashMap<&str, (bool, String, String)> = [
        ("up", (true, long_ax.clone(), short_ax.clone())),
        ("down", (false, long_ax.clone(), short_ax.clone())),
        ("right", (true, short_ax.clone(), long_ax.clone())),
        ("left", (false, short_ax.clone(), long_ax.clone())),
    ]
    .into_iter()
    .collect();
    table
        .get(direction)
        .map(|(hinge_side_max, edge_axis, extend_axis)| CornerDir {
            hinge_side_max: *hinge_side_max,
            edge_axis: edge_axis.clone(),
            extend_axis: extend_axis.clone(),
        })
}

pub fn create_corner(script_text: &str, direction: &str) -> String {
    let pair = match detect_constant_pair(script_text) {
        Some(p) => p,
        None => return script_text.to_string(),
    };

    let const_axis = pair.axis.clone();
    let free_axes: Vec<String> = ["X", "Y", "Z"]
        .iter()
        .filter(|a| **a != const_axis)
        .map(|s| s.to_string())
        .collect();
    let dir_params = match direction_to_params(direction, &free_axes) {
        Some(d) => d,
        None => return script_text.to_string(),
    };

    let lines = parse_script(script_text);

    let hinge_side_max = dir_params.hinge_side_max;
    let edge_axis = dir_params.edge_axis.clone();
    let extend_axis = dir_params.extend_axis.clone();

    // Structural inference (hinge, frame, thickness) uses material geometry
    // only. Boundary-condition and empty boxes are still transformed in the
    // output pass below, but they must not define the corner dimensions.
    let elements: Vec<&ScriptLine> = lines
        .iter()
        .filter(|l| l.segment.is_some() && l.label.as_deref() == Some("p"))
        .collect();
    if elements.is_empty() {
        return script_text.to_string();
    }

    let mut all_coords: HashMap<String, Vec<f64>> = [
        ("X".to_string(), Vec::new()),
        ("Y".to_string(), Vec::new()),
        ("Z".to_string(), Vec::new()),
    ]
    .into_iter()
    .collect();
    for line in &elements {
        let seg = line.segment.as_ref().unwrap();
        let (ca1, ca2) = seg_axis(seg, &const_axis);
        all_coords
            .get_mut(&const_axis)
            .unwrap()
            .extend_from_slice(&[ca1, ca2]);
        let (ea1, ea2) = seg_axis(seg, &edge_axis);
        all_coords
            .get_mut(&edge_axis)
            .unwrap()
            .extend_from_slice(&[ea1, ea2]);
        let (xa1, xa2) = seg_axis(seg, &extend_axis);
        all_coords
            .get_mut(&extend_axis)
            .unwrap()
            .extend_from_slice(&[xa1, xa2]);
    }

    let c_min = all_coords[&const_axis]
        .iter()
        .cloned()
        .fold(f64::INFINITY, f64::min);
    let c_max = all_coords[&const_axis]
        .iter()
        .cloned()
        .fold(f64::NEG_INFINITY, f64::max);

    let hinge_val = if hinge_side_max {
        all_coords[&edge_axis]
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max)
    } else {
        all_coords[&edge_axis]
            .iter()
            .cloned()
            .fold(f64::INFINITY, f64::min)
    };

    let at_hinge = |seg: &Segment| -> bool {
        let (ev1, ev2) = seg_axis(seg, &edge_axis);
        let (lo, hi) = if ev1 <= ev2 { (ev1, ev2) } else { (ev2, ev1) };
        lo <= hinge_val + TOL && hi >= hinge_val - TOL
    };

    let hinge_elements: Vec<&&ScriptLine> = elements
        .iter()
        .filter(|l| at_hinge(l.segment.as_ref().unwrap()))
        .collect();
    if hinge_elements.is_empty() {
        return script_text.to_string();
    }

    let frame = hinge_elements
        .iter()
        .max_by(|a, b| {
            let fa = seg_axis(a.segment.as_ref().unwrap(), &extend_axis);
            let fb = seg_axis(b.segment.as_ref().unwrap(), &extend_axis);
            let da = (fa.1 - fa.0).abs();
            let db = (fb.1 - fb.0).abs();
            // Python max() returns the FIRST maximum — Rust max_by returns the LAST.
            // Reverse comparison to get first-max (reverse-less means greater).
            // a > b when da > db, but when equal we want a first.
            // Use da.partial_cmp(&db) and then .then_with(|| Ordering::Less)
            // so that when equal, a < b (keeps a before b).
            match da.partial_cmp(&db).unwrap() {
                std::cmp::Ordering::Equal => std::cmp::Ordering::Greater, // keep a first
                other => other,
            }
        })
        .unwrap();

    let fseg = frame.segment.as_ref().unwrap();
    let (fev1, fev2) = seg_axis(fseg, &extend_axis);
    let corner_thickness = (fev2 - fev1).abs();

    let trim_val = if hinge_side_max {
        c_max - corner_thickness
    } else {
        c_min + corner_thickness
    };
    let extend_bound = if hinge_side_max { c_max } else { c_min };

    let mut result: Vec<ScriptLine> = Vec::new();
    for line in &lines {
        let seg = match &line.segment {
            None => {
                result.push(line.clone());
                continue;
            }
            Some(s) => *s,
        };

        let (cv1, cv2) = seg_axis(&seg, &const_axis);
        let (c_lo, c_hi) = if cv1 <= cv2 { (cv1, cv2) } else { (cv2, cv1) };
        let (ev1, ev2) = seg_axis(&seg, &edge_axis);
        let (e_lo, e_hi) = if ev1 <= ev2 { (ev1, ev2) } else { (ev2, ev1) };
        let (xv1, xv2) = seg_axis(&seg, &extend_axis);
        let (x_lo, x_hi) = if xv1 <= xv2 { (xv1, xv2) } else { (xv2, xv1) };

        let is_frame = same_seg(&seg, fseg);

        let (mut trimmed_c_hi, mut trimmed_c_lo) = (c_hi, c_lo);
        if hinge_side_max {
            if !is_frame && c_hi > trim_val + TOL {
                trimmed_c_hi = trim_val;
            }
        } else if !is_frame && c_lo < trim_val - TOL {
            trimmed_c_lo = trim_val;
        }

        let mut trimmed: Option<Segment> = None;
        if trimmed_c_lo < trimmed_c_hi - TOL {
            trimmed = Some(seg_set(
                &seg_set(&seg, &const_axis, trimmed_c_lo, trimmed_c_hi),
                &edge_axis,
                e_lo,
                e_hi,
            ));
            result.push(line_clone(line, trimmed.unwrap()));
        }

        let (copy_c_lo, copy_c_hi) = if is_frame {
            if hinge_side_max {
                (c_hi - corner_thickness, c_hi)
            } else {
                (c_lo, c_lo + corner_thickness)
            }
        } else {
            (trimmed_c_lo, trimmed_c_hi)
        };

        let (copy_x_lo, copy_x_hi) = if hinge_side_max {
            (x_lo, extend_bound)
        } else {
            (extend_bound, x_hi)
        };

        if copy_c_lo < copy_c_hi - TOL && copy_x_lo < copy_x_hi - TOL {
            let copy_seg = seg_set(
                &seg_set(&seg, &const_axis, copy_c_lo, copy_c_hi),
                &extend_axis,
                copy_x_lo,
                copy_x_hi,
            );
            if trimmed.is_none_or(|t| !same_seg(&copy_seg, &t)) {
                result.push(line_clone(line, copy_seg));
            }
        }
    }

    let result = deduplicate_corner_result(result);
    serialize_script(&result, crate::config::LINE_BREAK)
}

/// Removes redundant generated material boxes: exact duplicates and boxes fully
/// contained by another box with the same material metadata. Conservative:
/// only same-label, same-trailing `p` boxes are compared.
fn deduplicate_corner_result(lines: Vec<ScriptLine>) -> Vec<ScriptLine> {
    let mut keep = vec![true; lines.len()];
    for i in 0..lines.len() {
        if !keep[i] || lines[i].label.as_deref() != Some("p") {
            continue;
        }
        let Some(si) = lines[i].segment else {
            continue;
        };
        for j in (i + 1)..lines.len() {
            if !keep[j] || lines[j].label.as_deref() != Some("p") {
                continue;
            }
            if lines[i].trailing != lines[j].trailing {
                continue;
            }
            let Some(sj) = lines[j].segment else {
                continue;
            };
            if same_seg(&si, &sj) || seg_contains(&si, &sj) {
                keep[j] = false;
            } else if seg_contains(&sj, &si) {
                keep[i] = false;
                break;
            }
        }
    }
    lines
        .into_iter()
        .zip(keep)
        .filter_map(|(line, keep)| keep.then_some(line))
        .collect()
}

fn seg_contains(outer: &Segment, inner: &Segment) -> bool {
    let [ox1, oy1, oz1, ox2, oy2, oz2] = outer.as_tuple();
    let [ix1, iy1, iz1, ix2, iy2, iz2] = inner.as_tuple();
    let (ox_lo, ox_hi) = (ox1.min(ox2), ox1.max(ox2));
    let (oy_lo, oy_hi) = (oy1.min(oy2), oy1.max(oy2));
    let (oz_lo, oz_hi) = (oz1.min(oz2), oz1.max(oz2));
    let (ix_lo, ix_hi) = (ix1.min(ix2), ix1.max(ix2));
    let (iy_lo, iy_hi) = (iy1.min(iy2), iy1.max(iy2));
    let (iz_lo, iz_hi) = (iz1.min(iz2), iz1.max(iz2));
    ox_lo <= ix_lo + TOL
        && ox_hi >= ix_hi - TOL
        && oy_lo <= iy_lo + TOL
        && oy_hi >= iy_hi - TOL
        && oz_lo <= iz_lo + TOL
        && oz_hi >= iz_hi - TOL
}
