//! Проверка модели HEAT3: подсчёт плоскостей, поиск внутренних пустот, упрощение.

use std::collections::{HashMap, VecDeque};

use crate::models::Segment;
use crate::parser::{parse_script, serialize_script, ScriptLine};

pub const PLANE_LIMIT: usize = 150;
pub const CAVITY_CELL_LIMIT: usize = 1_000_000;
const TOL: f64 = 1e-9;

#[derive(Clone, Copy, Debug)]
pub struct PlaneUsage {
    pub x: usize,
    pub y: usize,
    pub z: usize,
    pub total_objects: usize,
    pub limit: usize,
}

impl PlaneUsage {
    pub fn x_exceeded(&self) -> bool {
        self.x > self.limit
    }
    pub fn y_exceeded(&self) -> bool {
        self.y > self.limit
    }
    pub fn z_exceeded(&self) -> bool {
        self.z > self.limit
    }
    pub fn any_exceeded(&self) -> bool {
        self.x_exceeded() || self.y_exceeded() || self.z_exceeded()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Box3D {
    pub x1: f64,
    pub y1: f64,
    pub z1: f64,
    pub x2: f64,
    pub y2: f64,
    pub z2: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct Cavity {
    pub bounds: [f64; 6],
    pub cell_count: usize,
}

#[derive(Clone, Debug)]
pub struct CavityCheckResult {
    pub cavities: Vec<Cavity>,
    pub skipped: bool,
    pub cell_count: usize,
    pub cell_limit: usize,
}

impl CavityCheckResult {
    pub fn cavities_found(&self) -> bool {
        !self.cavities.is_empty()
    }
}

fn normalize_plane(value: f64) -> String {
    // Python: f"{value:.10g}" с обнулением значений < 1e-12
    let v = if value.abs() < 1e-12 { 0.0 } else { value };
    crate::text::format_g(v, 10)
}

pub fn count_model_planes(script_text: &str) -> PlaneUsage {
    let mut x_planes: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut y_planes: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut z_planes: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut total_objects = 0usize;

    for line in parse_script(script_text) {
        let segment = match line.segment {
            None => continue,
            Some(s) => s,
        };
        total_objects += 1;
        let [x1, y1, z1, x2, y2, z2] = segment.as_tuple();
        x_planes.insert(normalize_plane(x1));
        x_planes.insert(normalize_plane(x2));
        y_planes.insert(normalize_plane(y1));
        y_planes.insert(normalize_plane(y2));
        z_planes.insert(normalize_plane(z1));
        z_planes.insert(normalize_plane(z2));
    }

    PlaneUsage {
        x: x_planes.len(),
        y: y_planes.len(),
        z: z_planes.len(),
        total_objects,
        limit: PLANE_LIMIT,
    }
}

pub fn format_plane_usage(usage: &PlaneUsage) -> String {
    format!(
        "X: {} из {}\nY: {} из {}\nZ: {} из {}\nОбъектов учтено: {}",
        usage.x, usage.limit, usage.y, usage.limit, usage.z, usage.limit, usage.total_objects
    )
}

struct CavityGeometry {
    plane_boxes: Vec<Box3D>,
    occupancy_operations: Vec<(bool, Box3D)>,
}

fn collect_cavity_geometry(script_text: &str) -> CavityGeometry {
    let mut plane_boxes = Vec::new();
    let mut occupancy_operations = Vec::new();
    for line in parse_script(script_text) {
        let label = line.label.as_deref();
        if !matches!(label, Some("p" | "b" | "e")) {
            continue;
        }
        let segment = match line.segment {
            None => continue,
            Some(s) => s,
        };
        let [x1, y1, z1, x2, y2, z2] = segment.as_tuple();
        let min_x = x1.min(x2);
        let max_x = x1.max(x2);
        let min_y = y1.min(y2);
        let max_y = y1.max(y2);
        let min_z = z1.min(z2);
        let max_z = z1.max(z2);
        if (max_x - min_x).abs() < 1e-12
            || (max_y - min_y).abs() < 1e-12
            || (max_z - min_z).abs() < 1e-12
        {
            continue;
        }
        let box_ = Box3D {
            x1: min_x,
            y1: min_y,
            z1: min_z,
            x2: max_x,
            y2: max_y,
            z2: max_z,
        };
        // HEAT3 applies overlapping objects in script order: a later material
        // box fills cells, while an `e` box cuts them out. BC boxes contribute
        // mesh planes, but are surfaces rather than material volume.
        plane_boxes.push(box_);
        match label {
            Some("p") => occupancy_operations.push((true, box_)),
            Some("e") => occupancy_operations.push((false, box_)),
            _ => {}
        }
    }
    CavityGeometry {
        plane_boxes,
        occupancy_operations,
    }
}

fn unique_sorted_coords(boxes: &[Box3D], axis: char) -> Vec<f64> {
    let mut values_by_key: HashMap<String, f64> = HashMap::new();
    for box_ in boxes {
        let (a, b) = match axis {
            'x' => (box_.x1, box_.x2),
            'y' => (box_.y1, box_.y2),
            'z' => (box_.z1, box_.z2),
            _ => (0.0, 0.0),
        };
        values_by_key.insert(normalize_plane(a), a);
        values_by_key.insert(normalize_plane(b), b);
    }
    let mut vals: Vec<f64> = values_by_key.into_values().collect();
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
    vals
}

fn cell_index(ix: usize, iy: usize, iz: usize, ny: usize, nz: usize) -> usize {
    ((ix * ny) + iy) * nz + iz
}

fn neighbors(
    ix: usize,
    iy: usize,
    iz: usize,
    nx: usize,
    ny: usize,
    nz: usize,
) -> Vec<(usize, usize, usize)> {
    let mut out = Vec::new();
    for (dx, dy, dz) in [
        (-1, 0, 0),
        (1, 0, 0),
        (0, -1, 0),
        (0, 1, 0),
        (0, 0, -1),
        (0, 0, 1),
    ] {
        let next_ix = ix as isize + dx;
        let next_iy = iy as isize + dy;
        let next_iz = iz as isize + dz;
        if next_ix >= 0
            && next_ix < nx as isize
            && next_iy >= 0
            && next_iy < ny as isize
            && next_iz >= 0
            && next_iz < nz as isize
        {
            out.push((next_ix as usize, next_iy as usize, next_iz as usize));
        }
    }
    out
}

pub fn detect_internal_cavities(script_text: &str, cell_limit: usize) -> CavityCheckResult {
    let geometry = collect_cavity_geometry(script_text);
    if geometry.plane_boxes.is_empty() || geometry.occupancy_operations.is_empty() {
        return CavityCheckResult {
            cavities: Vec::new(),
            skipped: false,
            cell_count: 0,
            cell_limit,
        };
    }

    let x_coords = unique_sorted_coords(&geometry.plane_boxes, 'x');
    let y_coords = unique_sorted_coords(&geometry.plane_boxes, 'y');
    let z_coords = unique_sorted_coords(&geometry.plane_boxes, 'z');
    let nx = (x_coords.len() as isize - 1).max(0) as usize;
    let ny = (y_coords.len() as isize - 1).max(0) as usize;
    let nz = (z_coords.len() as isize - 1).max(0) as usize;
    let cell_count = nx * ny * nz;

    if cell_count == 0 {
        return CavityCheckResult {
            cavities: Vec::new(),
            skipped: false,
            cell_count: 0,
            cell_limit,
        };
    }
    if cell_count > cell_limit {
        return CavityCheckResult {
            cavities: Vec::new(),
            skipped: true,
            cell_count,
            cell_limit,
        };
    }

    let x_index: HashMap<String, usize> = x_coords
        .iter()
        .enumerate()
        .map(|(i, v)| (normalize_plane(*v), i))
        .collect();
    let y_index: HashMap<String, usize> = y_coords
        .iter()
        .enumerate()
        .map(|(i, v)| (normalize_plane(*v), i))
        .collect();
    let z_index: HashMap<String, usize> = z_coords
        .iter()
        .enumerate()
        .map(|(i, v)| (normalize_plane(*v), i))
        .collect();
    let mut occupied = vec![0u8; cell_count];

    for (is_material, box_) in &geometry.occupancy_operations {
        let ix1 = x_index[&normalize_plane(box_.x1)];
        let ix2 = x_index[&normalize_plane(box_.x2)];
        let iy1 = y_index[&normalize_plane(box_.y1)];
        let iy2 = y_index[&normalize_plane(box_.y2)];
        let iz1 = z_index[&normalize_plane(box_.z1)];
        let iz2 = z_index[&normalize_plane(box_.z2)];
        for ix in ix1..ix2 {
            for iy in iy1..iy2 {
                let base = (ix * ny + iy) * nz;
                for iz in iz1..iz2 {
                    occupied[base + iz] = u8::from(*is_material);
                }
            }
        }
    }

    let mut visited = vec![0u8; cell_count];

    fn flood_external(
        start: (usize, usize, usize),
        occupied: &[u8],
        visited: &mut [u8],
        nx: usize,
        ny: usize,
        nz: usize,
    ) {
        let mut queue = VecDeque::new();
        queue.push_back(start);
        while let Some((ix, iy, iz)) = queue.pop_front() {
            let index = cell_index(ix, iy, iz, ny, nz);
            if visited[index] != 0 || occupied[index] != 0 {
                continue;
            }
            visited[index] = 1;
            for n in neighbors(ix, iy, iz, nx, ny, nz) {
                queue.push_back(n);
            }
        }
    }

    for ix in 0..nx {
        for iy in 0..ny {
            for iz in 0..nz {
                if !(ix == 0 || ix == nx - 1 || iy == 0 || iy == ny - 1 || iz == 0 || iz == nz - 1)
                {
                    continue;
                }
                let index = cell_index(ix, iy, iz, ny, nz);
                if occupied[index] == 0 && visited[index] == 0 {
                    flood_external((ix, iy, iz), &occupied, &mut visited, nx, ny, nz);
                }
            }
        }
    }

    let mut cavities: Vec<Cavity> = Vec::new();
    for ix in 0..nx {
        for iy in 0..ny {
            for iz in 0..nz {
                let index = cell_index(ix, iy, iz, ny, nz);
                if visited[index] != 0 || occupied[index] != 0 {
                    continue;
                }
                let mut queue = VecDeque::new();
                queue.push_back((ix, iy, iz));
                let mut min_ix = ix;
                let mut max_ix = ix;
                let mut min_iy = iy;
                let mut max_iy = iy;
                let mut min_iz = iz;
                let mut max_iz = iz;
                let mut component_cells = 0usize;
                while let Some((cell_ix, cell_iy, cell_iz)) = queue.pop_front() {
                    let cell_index = cell_index(cell_ix, cell_iy, cell_iz, ny, nz);
                    if visited[cell_index] != 0 || occupied[cell_index] != 0 {
                        continue;
                    }
                    visited[cell_index] = 1;
                    component_cells += 1;
                    min_ix = min_ix.min(cell_ix);
                    max_ix = max_ix.max(cell_ix);
                    min_iy = min_iy.min(cell_iy);
                    max_iy = max_iy.max(cell_iy);
                    min_iz = min_iz.min(cell_iz);
                    max_iz = max_iz.max(cell_iz);
                    for n in neighbors(cell_ix, cell_iy, cell_iz, nx, ny, nz) {
                        queue.push_back(n);
                    }
                }
                cavities.push(Cavity {
                    bounds: [
                        x_coords[min_ix],
                        y_coords[min_iy],
                        z_coords[min_iz],
                        x_coords[max_ix + 1],
                        y_coords[max_iy + 1],
                        z_coords[max_iz + 1],
                    ],
                    cell_count: component_cells,
                });
            }
        }
    }

    CavityCheckResult {
        cavities,
        skipped: false,
        cell_count,
        cell_limit,
    }
}

fn format_coord(value: f64) -> String {
    normalize_plane(value)
}

pub fn format_cavity_check(result: &CavityCheckResult) -> String {
    if result.skipped {
        return format!(
            "Проверка пустот пропущена: сетка {} ячеек превышает лимит {}.",
            result.cell_count, result.cell_limit
        );
    }
    if result.cavities.is_empty() {
        return "Пустоты: не обнаружены".to_string();
    }
    let mut lines = vec![format!(
        "Пустоты: обнаружено {} областей",
        result.cavities.len()
    )];
    for (index, cavity) in result.cavities.iter().enumerate() {
        let [x1, y1, z1, x2, y2, z2] = cavity.bounds;
        lines.push(format!(
            "{}. X {}..{}, Y {}..{}, Z {}..{}",
            index + 1,
            format_coord(x1),
            format_coord(x2),
            format_coord(y1),
            format_coord(y2),
            format_coord(z1),
            format_coord(z2)
        ));
    }
    lines.join("\n")
}

const AXIS_NAMES: [&str; 3] = ["X", "Y", "Z"];

fn axis_vals(segment: &Segment, axis: usize) -> (f64, f64) {
    let t = segment.as_tuple();
    (t[axis], t[axis + 3])
}

fn replace_axis_val(segment: &Segment, axis: usize, old: f64, new: f64) -> Segment {
    let mut vals = segment.as_tuple();
    if (vals[axis] - old).abs() < TOL {
        vals[axis] = new;
    }
    if (vals[axis + 3] - old).abs() < TOL {
        vals[axis + 3] = new;
    }
    Segment::new(vals[0], vals[1], vals[2], vals[3], vals[4], vals[5])
}

fn eval_merge(
    lines: &[ScriptLine],
    axis: usize,
    old_val: f64,
    new_val: f64,
    max_change_ratio: f64,
) -> (bool, usize) {
    let mut affected = 0usize;
    for line in lines {
        let segment = match &line.segment {
            None => continue,
            Some(s) => s,
        };
        let (v1, v2) = axis_vals(segment, axis);
        let mut hit = false;
        let old_dim = (v2 - v1).abs();
        if (v1 - old_val).abs() < TOL {
            hit = true;
        }
        if (v2 - old_val).abs() < TOL {
            hit = true;
        }
        if !hit {
            continue;
        }
        affected += 1;
        if old_dim > TOL {
            // Python: оба if проверяются последовательно; если оба хита,
            // второй if (v2) перезаписывает new_dim.
            let new_dim = if (v2 - old_val).abs() < TOL {
                (v1 - new_val).abs()
            } else if (v1 - old_val).abs() < TOL {
                (v2 - new_val).abs()
            } else {
                // unreachable: hit уже был бы false
                0.0
            };
            let change = (new_dim - old_dim).abs() / old_dim;
            if change > max_change_ratio + TOL {
                return (false, 0);
            }
        }
    }
    (true, affected)
}

fn apply_merge(lines: &mut [ScriptLine], axis: usize, old_val: f64, new_val: f64) {
    for line in lines {
        let segment = match &line.segment {
            None => continue,
            Some(s) => *s,
        };
        let new_seg = replace_axis_val(&segment, axis, old_val, new_val);
        if new_seg != segment {
            line.segment = Some(new_seg);
        }
    }
}

pub fn extract_material(trailing: &str) -> String {
    for sep in [";", "//", "#"] {
        if let Some(idx) = trailing.find(sep) {
            return trailing[idx + sep.len()..].trim().to_string();
        }
    }
    trailing.trim().to_string()
}

#[derive(Clone, Debug)]
pub struct ElementChangeInfo {
    pub index: usize,
    pub material: String,
    pub axis: String,
    pub original_text: String,
    pub coord1: f64,
    pub coord2: f64,
    pub old_val: f64,
    pub new_val: f64,
}

#[derive(Clone, Debug)]
pub struct MergeProposal {
    pub axis: String,
    pub coord_from: f64,
    pub coord_to: f64,
    pub gap: f64,
    pub changes: Vec<ElementChangeInfo>,
}

impl MergeProposal {
    pub fn affected_count(&self) -> usize {
        self.changes.len()
    }
}

#[derive(Clone, Debug)]
pub struct SimplifyAnalysis {
    pub proposals: Vec<MergeProposal>,
    pub before_planes: PlaneUsage,
    pub after_planes: Option<PlaneUsage>,
}

#[derive(Clone, Debug)]
pub struct SimplifyProposalsResult {
    pub script: String,
    pub applied_count: usize,
    pub rejected: Vec<MergeProposal>,
}

impl SimplifyAnalysis {
    pub fn total_saved(&self) -> i32 {
        match &self.after_planes {
            None => 0,
            Some(after) => {
                (self.before_planes.x as i32 - after.x as i32)
                    + (self.before_planes.y as i32 - after.y as i32)
                    + (self.before_planes.z as i32 - after.z as i32)
            }
        }
    }
}

fn build_proposal(
    lines: &[ScriptLine],
    axis: usize,
    old_val: f64,
    new_val: f64,
    max_change_ratio: f64,
) -> Option<MergeProposal> {
    let axis_name = AXIS_NAMES[axis].to_string();
    let mut changes: Vec<ElementChangeInfo> = Vec::new();
    for (li, line) in lines.iter().enumerate() {
        let segment = match &line.segment {
            None => continue,
            Some(s) => s,
        };
        let (v1, v2) = axis_vals(segment, axis);
        let hit_val = if (v1 - old_val).abs() < TOL {
            Some(v1)
        } else if (v2 - old_val).abs() < TOL {
            Some(v2)
        } else {
            None
        };
        let hit_val = match hit_val {
            None => continue,
            Some(h) => h,
        };
        let other = if (v1 - old_val).abs() < TOL { v2 } else { v1 };
        let old_dim = (other - old_val).abs();
        let new_dim = (other - new_val).abs();
        if old_dim > TOL {
            let change = (new_dim - old_dim).abs() / old_dim;
            if change > max_change_ratio + TOL {
                return None;
            }
        }
        changes.push(ElementChangeInfo {
            index: li,
            material: extract_material(&line.trailing),
            axis: axis_name.clone(),
            original_text: line.raw.trim().to_string(),
            coord1: v1,
            coord2: v2,
            old_val: hit_val,
            new_val,
        });
    }
    Some(MergeProposal {
        axis: axis_name,
        coord_from: old_val,
        coord_to: new_val,
        gap: (new_val - old_val).abs(),
        changes,
    })
}

fn collect_merge_proposals(
    lines: &mut [ScriptLine],
    axis: usize,
    tolerance: f64,
    max_change_ratio: f64,
) -> Vec<MergeProposal> {
    let mut sorted_coords: Vec<f64> = Vec::new();
    let mut seen: std::collections::HashSet<u64> = std::collections::HashSet::new();
    for line in lines.iter() {
        if let Some(seg) = &line.segment {
            let (v1, v2) = axis_vals(seg, axis);
            for v in [v1, v2] {
                if seen.insert(v.to_bits()) {
                    sorted_coords.push(v);
                }
            }
        }
    }
    // total_cmp устойчив к NaN (в отличие от partial_cmp().unwrap()); парсер уже
    // отклоняет non-finite, но это глубокоэшелонированная защита.
    sorted_coords.sort_by(|a, b| a.total_cmp(b));

    let mut proposals: Vec<MergeProposal> = Vec::new();
    let mut i = 0;
    while i < sorted_coords.len().saturating_sub(1) {
        let c1 = sorted_coords[i];
        let c2 = sorted_coords[i + 1];
        let gap = c2 - c1;
        if gap > tolerance {
            i += 1;
            continue;
        }

        let down = build_proposal(lines, axis, c2, c1, max_change_ratio);
        let up = build_proposal(lines, axis, c1, c2, max_change_ratio);

        let chosen: Option<MergeProposal> = match (&down, &up) {
            (Some(d), Some(u)) => {
                if d.changes.len() <= u.changes.len() {
                    Some(d.clone())
                } else {
                    Some(u.clone())
                }
            }
            (Some(d), None) => Some(d.clone()),
            (None, Some(u)) => Some(u.clone()),
            (None, None) => None,
        };

        match chosen {
            Some(p) => {
                apply_merge(lines, axis, p.coord_from, p.coord_to);
                proposals.push(p);
                let remove_idx = if matches!(&down, Some(d) if d.changes.len() <= up.as_ref().map(|u| u.changes.len()).unwrap_or(usize::MAX))
                {
                    i + 1
                } else {
                    i
                };
                sorted_coords.remove(remove_idx);
            }
            None => {
                i += 1;
            }
        }
    }
    proposals
}

pub fn analyze_simplify(
    script_text: &str,
    tolerance: f64,
    max_change_percent: f64,
) -> SimplifyAnalysis {
    let before = count_model_planes(script_text);
    let tolerance_m = tolerance / 1000.0;
    let max_change_ratio = max_change_percent / 100.0;
    let mut lines = parse_script(script_text);

    let mut all_proposals: Vec<MergeProposal> = Vec::new();
    for axis_idx in 0..3 {
        let proposals =
            collect_merge_proposals(&mut lines, axis_idx, tolerance_m, max_change_ratio);
        all_proposals.extend(proposals);
    }

    let after_text = serialize_script(&lines, crate::config::LINE_BREAK);
    let after = if !all_proposals.is_empty() {
        Some(count_model_planes(&after_text))
    } else {
        Some(before)
    };

    SimplifyAnalysis {
        proposals: all_proposals,
        before_planes: before,
        after_planes: after,
    }
}

pub fn simplify_planes(script_text: &str, tolerance: f64, max_change_percent: f64) -> String {
    let tolerance_m = tolerance / 1000.0;
    let max_change_ratio = max_change_percent / 100.0;
    let mut lines = parse_script(script_text);
    for axis_idx in 0..3 {
        simplify_axis(&mut lines, axis_idx, tolerance_m, max_change_ratio);
    }
    serialize_script(&lines, crate::config::LINE_BREAK)
}

/// Replays displayed proposals in order, validating each against the model that
/// the preceding selected proposals actually produced. A proposal may be unsafe
/// when an earlier proposal it depended on was deselected.
pub fn simplify_proposals(
    script_text: &str,
    proposals: &[MergeProposal],
    tolerance: f64,
    max_change_percent: f64,
) -> SimplifyProposalsResult {
    let axis_index = [("X", 0), ("Y", 1), ("Z", 2)];
    let tolerance_m = tolerance / 1000.0;
    let max_change_ratio = max_change_percent / 100.0;
    let mut lines = parse_script(script_text);
    let mut applied_count = 0;
    let mut rejected = Vec::new();
    for prop in proposals {
        let Some(axis) = axis_index
            .iter()
            .find(|(name, _)| *name == prop.axis)
            .map(|(_, i)| *i)
        else {
            rejected.push(prop.clone());
            continue;
        };
        if prop.gap > tolerance_m + TOL
            || !prop.coord_from.is_finite()
            || !prop.coord_to.is_finite()
        {
            rejected.push(prop.clone());
            continue;
        }
        let Some(revalidated) = build_proposal(
            &lines,
            axis,
            prop.coord_from,
            prop.coord_to,
            max_change_ratio,
        ) else {
            rejected.push(prop.clone());
            continue;
        };
        if revalidated.changes.is_empty() {
            rejected.push(prop.clone());
            continue;
        }
        apply_merge(&mut lines, axis, prop.coord_from, prop.coord_to);
        applied_count += 1;
    }
    SimplifyProposalsResult {
        script: serialize_script(&lines, crate::config::LINE_BREAK),
        applied_count,
        rejected,
    }
}

fn simplify_axis(lines: &mut [ScriptLine], axis: usize, tolerance: f64, max_change_ratio: f64) {
    let mut unique: Vec<f64> = Vec::new();
    let mut seen: std::collections::HashSet<u64> = std::collections::HashSet::new();
    for line in lines.iter() {
        if let Some(seg) = &line.segment {
            let (v1, v2) = axis_vals(seg, axis);
            for v in [v1, v2] {
                if seen.insert(v.to_bits()) {
                    unique.push(v);
                }
            }
        }
    }
    let mut sorted_coords: Vec<f64> = unique;
    sorted_coords.sort_by(|a, b| a.total_cmp(b));

    let mut i = 0;
    while i < sorted_coords.len().saturating_sub(1) {
        let c1 = sorted_coords[i];
        let c2 = sorted_coords[i + 1];
        let gap = c2 - c1;
        if gap > tolerance {
            i += 1;
            continue;
        }

        let (ok_down, count_down) = eval_merge(lines, axis, c2, c1, max_change_ratio);
        let (ok_up, count_up) = eval_merge(lines, axis, c1, c2, max_change_ratio);

        if ok_down && (count_down <= count_up || !ok_up) {
            apply_merge(lines, axis, c2, c1);
            sorted_coords.remove(i + 1);
        } else if ok_up {
            apply_merge(lines, axis, c1, c2);
            sorted_coords.remove(i);
        } else {
            i += 1;
        }
    }
}
