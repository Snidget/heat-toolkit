//! Экспорт модели (набора параллелепипедов) в формат STEP (ISO 10303-21).
//!
//! Формирует корректный текстовый STEP-файл без внешних зависимостей.

use std::collections::HashMap;
use std::path::Path;

/// Deterministic full-precision real for STEP geometry. The shortest
/// round-trip `f64` representation keeps thin features at large offsets from
/// collapsing (unlike a fixed significant-digit formatter).
fn fmt_r(value: f64) -> String {
    let mut text = format!("{value}");
    if !text.contains(['.', 'e', 'E']) {
        text.push_str(".0");
    }
    text
}

/// HEAT3 stores model coordinates in metres; this STEP writer declares millimetres.
const METRES_TO_MILLIMETRES: f64 = 1_000.0;

fn box_in_step_millimetres(box_: [f64; 6]) -> [f64; 6] {
    box_.map(|coordinate| coordinate * METRES_TO_MILLIMETRES)
}

fn step_ref(n: usize) -> String {
    format!("#{}", n)
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(v: [f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn loop_normal(loop_pts: &[[f64; 3]]) -> [f64; 3] {
    let mut sx = 0.0;
    let mut sy = 0.0;
    let mut sz = 0.0;
    let count = loop_pts.len();
    for i in 0..count {
        let p = loop_pts[i];
        let q = loop_pts[(i + 1) % count];
        sx += p[1] * q[2] - p[2] * q[1];
        sy += p[2] * q[0] - p[0] * q[2];
        sz += p[0] * q[1] - p[1] * q[0];
    }
    [sx, sy, sz]
}

fn ccw_order_indices(corners: &[[f64; 3]], indices: &[usize], normal: [f64; 3]) -> Vec<usize> {
    let centroid: [f64; 3] = {
        let mut c = [0.0, 0.0, 0.0];
        for &i in indices {
            for axis in 0..3 {
                c[axis] += corners[i][axis];
            }
        }
        let n = indices.len() as f64;
        [c[0] / n, c[1] / n, c[2] / n]
    };
    let mut u = [0.0, 0.0, 0.0];
    for (axis, value) in u.iter_mut().enumerate() {
        *value = corners[indices[0]][axis] - centroid[axis];
    }
    let u_len = norm(u);
    if u_len < 1e-9 {
        return indices.to_vec();
    }
    for value in &mut u {
        *value /= u_len;
    }
    let v = cross(normal, u);
    let v_len = norm(v);
    if v_len < 1e-9 {
        return indices.to_vec();
    }
    let v = [v[0] / v_len, v[1] / v_len, v[2] / v_len];

    let mut order: Vec<usize> = indices.to_vec();
    order.sort_by(|&a, &b| {
        let da = [
            corners[a][0] - centroid[0],
            corners[a][1] - centroid[1],
            corners[a][2] - centroid[2],
        ];
        let db = [
            corners[b][0] - centroid[0],
            corners[b][1] - centroid[1],
            corners[b][2] - centroid[2],
        ];
        let xa = da[0] * u[0] + da[1] * u[1] + da[2] * u[2];
        let ya = da[0] * v[0] + da[1] * v[1] + da[2] * v[2];
        let xb = db[0] * u[0] + db[1] * u[1] + db[2] * u[2];
        let yb = db[0] * v[0] + db[1] * v[1] + db[2] * v[2];
        ya.atan2(xa).partial_cmp(&yb.atan2(xb)).unwrap()
    });
    order
}

struct StepWriter {
    lines: Vec<String>,
    counter: usize,
    direction_cache: HashMap<(i64, i64, i64), usize>,
}

impl StepWriter {
    fn new() -> Self {
        StepWriter {
            lines: Vec::new(),
            counter: 0,
            direction_cache: HashMap::new(),
        }
    }

    fn nid(&mut self) -> usize {
        self.counter += 1;
        self.counter
    }

    fn add(&mut self, body: &str) -> usize {
        let index = self.nid();
        self.lines.push(format!("{} = {};", step_ref(index), body));
        index
    }

    fn direction(&mut self, xyz: [f64; 3]) -> usize {
        let key = (
            (xyz[0] * 1e9).round() as i64,
            (xyz[1] * 1e9).round() as i64,
            (xyz[2] * 1e9).round() as i64,
        );
        if let Some(&id) = self.direction_cache.get(&key) {
            return id;
        }
        let id = self.add(&format!(
            "DIRECTION('',({},{},{}))",
            fmt_r(xyz[0]),
            fmt_r(xyz[1]),
            fmt_r(xyz[2])
        ));
        self.direction_cache.insert(key, id);
        id
    }
}

fn add_box(writer: &mut StepWriter, box_: [f64; 6], xdir: usize, ydir: usize) -> usize {
    let x0 = box_[0].min(box_[3]);
    let x1 = box_[0].max(box_[3]);
    let y0 = box_[1].min(box_[4]);
    let y1 = box_[1].max(box_[4]);
    let z0 = box_[2].min(box_[5]);
    let z1 = box_[2].max(box_[5]);

    let corners: [[f64; 3]; 8] = [
        [x0, y0, z0],
        [x1, y0, z0],
        [x1, y1, z0],
        [x0, y1, z0],
        [x0, y0, z1],
        [x1, y0, z1],
        [x1, y1, z1],
        [x0, y1, z1],
    ];

    let points: Vec<usize> = corners
        .iter()
        .map(|c| {
            writer.add(&format!(
                "CARTESIAN_POINT('',({},{},{}))",
                fmt_r(c[0]),
                fmt_r(c[1]),
                fmt_r(c[2])
            ))
        })
        .collect();
    let vertices: Vec<usize> = points
        .iter()
        .map(|&p| writer.add(&format!("VERTEX_POINT('',{})", step_ref(p))))
        .collect();

    let edge_defs: [(usize, usize); 12] = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ];
    let mut edge_ids: HashMap<(usize, usize), usize> = HashMap::new();
    for &(a, b) in &edge_defs {
        let dx = corners[b][0] - corners[a][0];
        let dy = corners[b][1] - corners[a][1];
        let dz = corners[b][2] - corners[a][2];
        let length = (dx * dx + dy * dy + dz * dz).sqrt();
        let axis = [dx / length, dy / length, dz / length];
        let axis_id = writer.direction(axis);
        let vector = writer.add(&format!(
            "VECTOR('',{}, {})",
            step_ref(axis_id),
            fmt_r(length)
        ));
        let line = writer.add(&format!(
            "LINE('',{},{})",
            step_ref(points[a]),
            step_ref(vector)
        ));
        let edge_curve = writer.add(&format!(
            "EDGE_CURVE('',{},{},{},.T.)",
            step_ref(vertices[a]),
            step_ref(vertices[b]),
            step_ref(line)
        ));
        edge_ids.insert((a, b), edge_curve);
    }

    let faces: [([usize; 4], [f64; 3]); 6] = [
        ([1, 5, 6, 2], [1.0, 0.0, 0.0]),
        ([0, 3, 7, 4], [-1.0, 0.0, 0.0]),
        ([3, 2, 6, 7], [0.0, 1.0, 0.0]),
        ([0, 4, 5, 1], [0.0, -1.0, 0.0]),
        ([4, 5, 6, 7], [0.0, 0.0, 1.0]),
        ([0, 3, 2, 1], [0.0, 0.0, -1.0]),
    ];

    let mut face_ids: Vec<usize> = Vec::new();
    for (face_corners, normal) in &faces {
        let ordered = ccw_order_indices(&corners, face_corners, *normal);
        let loop_indices = ordered.clone();
        let mut oriented_edges: Vec<usize> = Vec::new();
        for i in 0..loop_indices.len() {
            let u = loop_indices[i];
            let v = loop_indices[(i + 1) % loop_indices.len()];
            let (ec, orient) = if let Some(&id) = edge_ids.get(&(u, v)) {
                (id, ".T.")
            } else {
                (*edge_ids.get(&(v, u)).unwrap(), ".F.")
            };
            oriented_edges.push(writer.add(&format!(
                "ORIENTED_EDGE('',*,*,{},{})",
                step_ref(ec),
                orient
            )));
        }
        let edge_loop = writer.add(&format!(
            "EDGE_LOOP('',({}))",
            oriented_edges
                .iter()
                .map(|o| step_ref(*o))
                .collect::<Vec<_>>()
                .join(",")
        ));
        let face_bound = writer.add(&format!("FACE_BOUND('',{},.T.)", step_ref(edge_loop)));
        let ndir = writer.direction(*normal);
        let xref = if normal[0].abs() > 0.5 { ydir } else { xdir };
        let fx = corners[loop_indices[0]][0];
        let fy = corners[loop_indices[0]][1];
        let fz = corners[loop_indices[0]][2];
        let face_pt = writer.add(&format!(
            "CARTESIAN_POINT('',({},{},{}))",
            fmt_r(fx),
            fmt_r(fy),
            fmt_r(fz)
        ));
        let face_axis = writer.add(&format!(
            "AXIS2_PLACEMENT_3D('',{},{},{})",
            step_ref(face_pt),
            step_ref(ndir),
            step_ref(xref)
        ));
        let plane = writer.add(&format!("PLANE('',{})", step_ref(face_axis)));
        let loop_n = loop_normal(&loop_indices.iter().map(|&i| corners[i]).collect::<Vec<_>>());
        let same_sense =
            if loop_n[0] * normal[0] + loop_n[1] * normal[1] + loop_n[2] * normal[2] >= 0.0 {
                ".T."
            } else {
                ".F."
            };
        let face = writer.add(&format!(
            "ADVANCED_FACE('',({}),{},{})",
            step_ref(face_bound),
            step_ref(plane),
            same_sense
        ));
        face_ids.push(face);
    }

    let closed_shell = writer.add(&format!(
        "CLOSED_SHELL('',({}))",
        face_ids
            .iter()
            .map(|f| step_ref(*f))
            .collect::<Vec<_>>()
            .join(",")
    ));
    writer.add(&format!(
        "MANIFOLD_SOLID_BREP('',{})",
        step_ref(closed_shell)
    ))
}

const SOLID_EPSILON: f64 = 1e-9;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StepExportSummary {
    pub exported_boxes: usize,
    pub skipped_degenerate_boxes: usize,
}

pub fn is_exportable_solid(box_: &[f64; 6]) -> bool {
    (box_[3] - box_[0]).abs() > SOLID_EPSILON
        && (box_[4] - box_[1]).abs() > SOLID_EPSILON
        && (box_[5] - box_[2]).abs() > SOLID_EPSILON
}

/// HEAT3 geometry that cannot be represented as positive STEP material solids.
///
/// STEP export currently writes only positive `p` material boxes. Empty `e`
/// cut-outs (subtractive), `s` material boxes (origin+extent) and unsupported
/// geometric commands would silently produce physically false geometry, so
/// their presence must fail the export instead of dropping them.
pub fn step_export_blockers(script: &str) -> Vec<String> {
    let (items, mut blockers) = crate::model_check::supported_geometry(script);
    for item in &items {
        if matches!(item.label, "e" | "s") && !blockers.iter().any(|b| b == item.label) {
            blockers.push(item.label.to_owned());
        }
    }
    blockers
}

/// Collects exactly the material boxes (`p`) that STEP export can write.
pub fn exportable_p_boxes(script: &str) -> Vec<[f64; 6]> {
    let (items, _) = crate::model_check::supported_geometry(script);
    items
        .into_iter()
        .filter(|item| item.label == "p")
        .map(|item| item.segment.as_tuple())
        .collect()
}

pub fn write_step(path: &Path, boxes: &[[f64; 6]]) -> std::io::Result<StepExportSummary> {
    let exportable_boxes: Vec<[f64; 6]> =
        boxes.iter().copied().filter(is_exportable_solid).collect();
    if exportable_boxes.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "no exportable solid boxes",
        ));
    }
    let summary = StepExportSummary {
        exported_boxes: exportable_boxes.len(),
        skipped_degenerate_boxes: boxes.len() - exportable_boxes.len(),
    };
    let mut writer = StepWriter::new();

    let ctx = writer.add("APPLICATION_CONTEXT('')");
    let prod_ctx = writer.add(&format!(
        "PRODUCT_CONTEXT('',{},'mechanical')",
        step_ref(ctx)
    ));
    let product = writer.add(&format!(
        "PRODUCT('PovorotnikModel','Exported boxes','',({}))",
        step_ref(prod_ctx)
    ));
    let formation = writer.add(&format!(
        "PRODUCT_DEFINITION_FORMATION('','',{})",
        step_ref(product)
    ));
    let pdef_ctx = writer.add(&format!(
        "PRODUCT_DEFINITION_CONTEXT('',{},'design')",
        step_ref(ctx)
    ));
    let pdef = writer.add(&format!(
        "PRODUCT_DEFINITION('','',{},{})",
        step_ref(formation),
        step_ref(pdef_ctx)
    ));
    let pdef_shape = writer.add(&format!(
        "PRODUCT_DEFINITION_SHAPE('','',{})",
        step_ref(pdef)
    ));

    let length_unit =
        writer.add("( LENGTH_UNIT ( ) NAMED_UNIT ( * ) SI_UNIT ( .MILLI. , .METRE. ) )");
    let plane_angle_unit =
        writer.add("( NAMED_UNIT ( * ) PLANE_ANGLE_UNIT ( ) SI_UNIT ( $ , .RADIAN. ) )");
    let solid_angle_unit =
        writer.add("( NAMED_UNIT ( * ) SOLID_ANGLE_UNIT ( ) SI_UNIT ( $ , .STERADIAN. ) )");
    let xdir = writer.direction([1.0, 0.0, 0.0]);
    let ydir = writer.direction([0.0, 1.0, 0.0]);
    let uncertainty = writer.add(&format!(
        "UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-04),{},'','')",
        step_ref(length_unit)
    ));
    let geom_ctx = writer.add(&format!(
        "( GEOMETRIC_REPRESENTATION_CONTEXT ( 3 ) GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT ( ({}) ) GLOBAL_UNIT_ASSIGNED_CONTEXT ( ({},{},{}) ) REPRESENTATION_CONTEXT ( 'Context #1', '3D Context with UNIT and UNCERTAINTY' ) )",
        step_ref(uncertainty),
        step_ref(length_unit),
        step_ref(plane_angle_unit),
        step_ref(solid_angle_unit)
    ));

    let brep_ids: Vec<usize> = exportable_boxes
        .iter()
        .copied()
        .map(box_in_step_millimetres)
        .map(|b| add_box(&mut writer, b, xdir, ydir))
        .collect();
    let items = brep_ids
        .iter()
        .map(|i| step_ref(*i))
        .collect::<Vec<_>>()
        .join(",");
    let shape_repr = writer.add(&format!(
        "SHAPE_REPRESENTATION('',({}),{})",
        items,
        step_ref(geom_ctx)
    ));
    writer.add(&format!(
        "SHAPE_DEFINITION_REPRESENTATION({},{})",
        step_ref(pdef_shape),
        step_ref(shape_repr)
    ));

    let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    let file_name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
        .replace('\'', "''");
    let header = [
        "ISO-10303-21;",
        "HEADER;",
        "FILE_DESCRIPTION(('Povorotnik model export'),'2;1');",
        &format!(
            "FILE_NAME('{}','{}',(''),(''),'opencode','Povorotnik','');",
            file_name, now
        ),
        "FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));",
        "ENDSEC;",
        "DATA;",
    ];
    let footer = ["ENDSEC;", "END-ISO-10303-21;"];

    let mut out = String::new();
    out.push_str(&header.join("\n"));
    out.push('\n');
    out.push_str(&writer.lines.join("\n"));
    out.push('\n');
    out.push_str(&footer.join("\n"));
    out.push('\n');

    // Atomic replacement: write to a temp file in the destination directory,
    // flush, then replace the destination only on success so a truncate/partial
    // write cannot destroy an existing valid export.
    crate::material_sort::atomic_write_bytes(path, out.as_bytes())
        .map_err(std::io::Error::other)?;
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn entity_map(lines: &[String]) -> HashMap<String, String> {
        lines
            .iter()
            .filter_map(|line| {
                let (id, body) = line.split_once(" = ")?;
                Some((
                    id.trim().to_string(),
                    body.trim_end_matches(';').trim().to_string(),
                ))
            })
            .collect()
    }

    #[test]
    fn advanced_face_keeps_surface_outside_bounds_list() {
        let mut writer = StepWriter::new();
        let xdir = writer.direction([1.0, 0.0, 0.0]);
        let ydir = writer.direction([0.0, 1.0, 0.0]);

        add_box(&mut writer, [0.0, 0.0, 0.0, 1.0, 2.0, 3.0], xdir, ydir);

        let faces: Vec<&str> = writer
            .lines
            .iter()
            .filter(|line| line.contains("ADVANCED_FACE"))
            .map(String::as_str)
            .collect();

        assert_eq!(faces.len(), 6);
        for face in faces {
            let (_, arguments) = face
                .split_once("ADVANCED_FACE('',(")
                .expect("ADVANCED_FACE arguments");
            let (bounds, surface_and_sense) =
                arguments.split_once("),").expect("closed bounds list");

            assert!(bounds.starts_with('#'), "missing face bound: {face}");
            assert!(!bounds.contains(','), "surface leaked into bounds: {face}");
            assert!(
                surface_and_sense.starts_with('#'),
                "surface must be a separate reference: {face}"
            );
        }
    }

    #[test]
    fn lines_reference_cartesian_point_and_vector() {
        let mut writer = StepWriter::new();
        let xdir = writer.direction([1.0, 0.0, 0.0]);
        let ydir = writer.direction([0.0, 1.0, 0.0]);

        add_box(&mut writer, [0.0, 0.0, 0.0, 1.0, 2.0, 3.0], xdir, ydir);

        let lines: Vec<&str> = writer
            .lines
            .iter()
            .filter(|line| line.contains(" = LINE("))
            .map(String::as_str)
            .collect();
        let vectors = writer
            .lines
            .iter()
            .filter(|line| line.contains(" = VECTOR("))
            .count();
        let entities = entity_map(&writer.lines);

        assert_eq!(lines.len(), 12);
        assert_eq!(vectors, 12);
        for line in lines {
            let (_, arguments) = line
                .split_once("LINE('',")
                .expect("LINE should contain two references");
            let parts = arguments
                .trim_end_matches(");")
                .split(',')
                .collect::<Vec<_>>();
            assert_eq!(parts.len(), 2, "unexpected LINE arguments: {line}");
            assert!(
                entities[parts[0]].starts_with("CARTESIAN_POINT("),
                "LINE must start from CARTESIAN_POINT: {line}"
            );
            assert!(
                entities[parts[1]].starts_with("VECTOR("),
                "LINE must use VECTOR direction: {line}"
            );
        }
    }

    #[test]
    fn step_geometry_converts_heat3_metres_to_millimetres() {
        assert_eq!(
            box_in_step_millimetres([0.0, 0.0, 0.0, 1.0, 0.5, 0.1]),
            [0.0, 0.0, 0.0, 1000.0, 500.0, 100.0]
        );
    }

    #[test]
    fn step_export_fails_closed_on_empty_and_s_material_geometry() {
        // `p` + `e` cut-out: the empty box cannot be represented as a solid, so
        // exporting only `p` would be physically false.
        assert_eq!(
            step_export_blockers("p 0 0 0 3 3 3 Material\ne 1 1 1 2 2 2"),
            vec!["e".to_owned()]
        );
        // Official `s` material box is unsupported material geometry.
        assert_eq!(
            step_export_blockers("p 0 0 0 1 1 1 A\ns 0 0 0 1 1 1 B"),
            vec!["s".to_owned()]
        );
        // A pure `p` model is exportable.
        assert!(step_export_blockers("p 0 0 0 1 1 1 A\nb 0 0 0 1 1 1 2").is_empty());
        assert_eq!(exportable_p_boxes("p 0 0 0 1 2 3 A").len(), 1);
    }

    #[test]
    fn failed_export_preserves_an_existing_destination_file() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("heat3-step-atomic-{unique}.stp"));
        std::fs::write(&path, b"previous valid export").unwrap();

        // All-degenerate input fails before writing anything.
        let error = write_step(&path, &[[0.0, 0.0, 0.0, 0.0, 1.0, 1.0]]).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        assert_eq!(std::fs::read(&path).unwrap(), b"previous valid export");

        // A successful export atomically replaces it.
        write_step(&path, &[[0.0, 0.0, 0.0, 1.0, 2.0, 3.0]]).unwrap();
        let output = std::fs::read_to_string(&path).unwrap();
        assert!(output.contains("ISO-10303-21;"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn exported_step_keeps_a_thin_feature_at_a_large_offset() {
        // A 0.0001 m feature offset by 1000 m becomes 0.1 mm at 1_000_000 mm.
        // Validate the serialized geometry, not only the number formatter.
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("heat3-step-thin-{unique}.stp"));
        write_step(&path, &[[1000.0, 2.0, 3.0, 1000.0001, 4.0, 5.0]]).unwrap();

        let output = fs::read_to_string(&path).unwrap();
        let x_coordinates = output
            .lines()
            .filter_map(|line| {
                line.split_once("CARTESIAN_POINT('',(")
                    .map(|(_, point)| point)
            })
            .filter_map(|point| {
                point
                    .split_once(',')
                    .map(|(x, _)| x.parse::<f64>().unwrap())
            })
            .collect::<Vec<_>>();
        assert!(x_coordinates
            .iter()
            .any(|x| (*x - 1_000_000.0).abs() < 1e-9));
        assert!(x_coordinates
            .iter()
            .any(|x| (*x - 1_000_000.1).abs() < 1e-9));
        let minimum = x_coordinates.iter().copied().fold(f64::INFINITY, f64::min);
        let maximum = x_coordinates
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        assert!((maximum - minimum - 0.1).abs() < 1e-9);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn write_step_skips_degenerate_boxes_and_emits_valid_context() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("heat3-step-{unique}.stp"));

        let summary = write_step(
            &path,
            &[
                [0.0, 0.0, 0.0, 1.0, 2.0, 3.0],
                [2.0, 2.0, 2.0, 2.0, 4.0, 5.0],
            ],
        )
        .unwrap();

        let output = fs::read_to_string(&path).unwrap();
        let _ = fs::remove_file(&path);

        assert_eq!(output.matches("MANIFOLD_SOLID_BREP").count(), 1);
        assert_eq!(summary.exported_boxes, 1);
        assert_eq!(summary.skipped_degenerate_boxes, 1);
        assert!(output.contains("GEOMETRIC_REPRESENTATION_CONTEXT ( 3 )"));
        assert!(output.contains("GLOBAL_UNIT_ASSIGNED_CONTEXT"));
        assert!(output.contains("SI_UNIT ( .MILLI. , .METRE. )"));
        assert!(output.contains("LENGTH_MEASURE(1.E-04)"));
        assert!(output.contains("CARTESIAN_POINT('',(1000.0,2000.0,3000.0))"));
        assert!(output.contains("PRODUCT_DEFINITION_CONTEXT('',#1,'design')"));
    }

    #[test]
    fn write_step_escapes_apostrophe_in_file_name() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("heat3-model's-{unique}.stp"));

        write_step(&path, &[[0.0, 0.0, 0.0, 1.0, 2.0, 3.0]]).unwrap();

        let output = fs::read_to_string(&path).unwrap();
        let _ = fs::remove_file(&path);
        assert!(
            output.contains("FILE_NAME('heat3-model''s-"),
            "STEP string literal must escape apostrophes: {output}"
        );
    }

    #[test]
    fn write_step_rejects_an_all_degenerate_model_without_creating_a_file() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("heat3-degenerate-{unique}.stp"));

        let error = write_step(&path, &[[0.0, 0.0, 0.0, 0.0, 1.0, 1.0]]).unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        assert!(!path.exists());
    }
}
