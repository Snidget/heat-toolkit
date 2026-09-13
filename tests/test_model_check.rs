use heat3_povorotnik::model_check::{
    count_model_planes, detect_internal_cavities, format_cavity_check, simplify_proposals,
    MergeProposal,
};

fn box_line(x1: i32, y1: i32, z1: i32, x2: i32, y2: i32, z2: i32, label: &str) -> String {
    format!("{label}\t{x1}\t{y1}\t{z1}\t{x2}\t{y2}\t{z2}\tM\t! box")
}

fn unit_cube_shell_script(open_to_outside: bool) -> String {
    let mut lines = Vec::new();
    for x in 0..3 {
        for y in 0..3 {
            for z in 0..3 {
                if (x, y, z) == (1, 1, 1) {
                    continue;
                }
                if open_to_outside && (x, y, z) == (1, 1, 0) {
                    continue;
                }
                lines.push(box_line(x, y, z, x + 1, y + 1, z + 1, "p"));
            }
        }
    }
    lines.join("\r\n")
}

#[test]
fn test_count_model_planes_counts_unique_coordinates() {
    let script = "p\t0\t0\t0\t1\t1\t1\tМатериал 1\t! material box\r\np\t1\t0\t0\t2\t1\t1\tМатериал 2\t! material box\r\nb\t0\t1\t0\t1\t2\t1\t2\t! BC box\r\ne\t0\t0\t1\t1\t1\t2\t! empty box\r\nr 0 0 1 1 2D line\r\n! comment";
    let usage = count_model_planes(script);
    assert_eq!(usage.x, 3);
    assert_eq!(usage.y, 3);
    assert_eq!(usage.z, 3);
    assert_eq!(usage.total_objects, 4);
}

#[test]
fn test_count_model_planes_treats_reversed_coordinates_as_same_planes() {
    let script =
        "p\t2\t1\t1\t1\t0\t0\tМатериал 1\t! material box\r\nb\t1\t0\t0\t0\t1\t1\t2\t! BC box";
    let usage = count_model_planes(script);
    assert_eq!(usage.x, 3);
    assert_eq!(usage.y, 2);
    assert_eq!(usage.z, 2);
}

#[test]
fn test_count_model_planes_ignores_malformed_lines() {
    let script =
        "p\t0\t0\t0\t1\t1\tbroken\r\nnot a box\r\np\t0\t0\t0\t1\t1\t1\tМатериал\t! material box";
    let usage = count_model_planes(script);
    assert_eq!(usage.x, 2);
    assert_eq!(usage.y, 2);
    assert_eq!(usage.z, 2);
    assert_eq!(usage.total_objects, 1);
}

#[test]
fn test_count_model_planes_exceeds_limit_on_x_axis() {
    let lines: Vec<String> = (0..150)
        .map(|i| box_line(i, 0, 0, i + 1, 1, 1, "p"))
        .collect();
    let script = lines.join("\r\n");
    let usage = count_model_planes(&script);
    assert_eq!(usage.x, 151);
    assert!(usage.x_exceeded());
    assert!(!usage.y_exceeded());
    assert!(!usage.z_exceeded());
}

#[test]
fn test_detect_internal_cavities_finds_closed_empty_volume() {
    let result = detect_internal_cavities(&unit_cube_shell_script(false), 1_000_000);
    assert!(!result.skipped);
    assert_eq!(result.cavities.len(), 1);
    assert!((result.cavities[0].bounds[0] - 1.0).abs() < 1e-9);
    assert!((result.cavities[0].bounds[1] - 1.0).abs() < 1e-9);
    assert!((result.cavities[0].bounds[2] - 1.0).abs() < 1e-9);
    assert!((result.cavities[0].bounds[3] - 2.0).abs() < 1e-9);
    assert!((result.cavities[0].bounds[4] - 2.0).abs() < 1e-9);
    assert!((result.cavities[0].bounds[5] - 2.0).abs() < 1e-9);
}

#[test]
fn test_detect_internal_cavities_ignores_volume_connected_to_outside() {
    let result = detect_internal_cavities(&unit_cube_shell_script(true), 1_000_000);
    assert!(!result.skipped);
    assert!(result.cavities.is_empty());
    assert_eq!(format_cavity_check(&result), "Пустоты: не обнаружены");
}

#[test]
fn test_detect_internal_cavities_treats_empty_box_as_an_enclosed_cutout() {
    let script = "p 0 0 0 3 3 3 Material\ne 1 1 1 2 2 2";

    let result = detect_internal_cavities(script, 1_000_000);

    assert_eq!(result.cavities.len(), 1);
    assert_eq!(result.cavities[0].bounds, [1.0, 1.0, 1.0, 2.0, 2.0, 2.0]);
}

#[test]
fn test_detect_internal_cavities_treats_exterior_connected_cutout_as_open() {
    let script = "p 0 0 0 3 3 3 Material\ne 0 1 1 2 2 2";

    let result = detect_internal_cavities(script, 1_000_000);

    assert!(result.cavities.is_empty());
}

#[test]
fn test_detect_internal_cavities_does_not_fill_cutout_with_bc_box() {
    let script = "p 0 0 0 3 3 3 Material\ne 1 1 1 2 2 2\nb 1 1 1 2 2 2 2";

    let result = detect_internal_cavities(script, 1_000_000);

    assert_eq!(result.cavities.len(), 1);
    assert_eq!(result.cavities[0].bounds, [1.0, 1.0, 1.0, 2.0, 2.0, 2.0]);
}

#[test]
fn test_detect_internal_cavities_applies_material_and_empty_boxes_in_script_order() {
    let script = "p 0 0 0 3 3 3 Material\ne 1 1 1 2 2 2\np 1 1 1 2 2 2 Material";

    let result = detect_internal_cavities(script, 1_000_000);

    assert!(result.cavities.is_empty());
}

#[test]
fn test_detect_internal_cavities_skips_large_grid() {
    let result = detect_internal_cavities(&unit_cube_shell_script(false), 10);
    assert!(result.skipped);
    assert!(result.cavities.is_empty());
    assert_eq!(result.cell_count, 27);
}

fn merge_proposal(axis: &str, coord_from: f64, coord_to: f64) -> MergeProposal {
    MergeProposal {
        axis: axis.to_owned(),
        coord_from,
        coord_to,
        gap: (coord_to - coord_from).abs(),
        changes: Vec::new(),
    }
}

#[test]
fn test_simplify_proposals_revalidates_when_a_prerequisite_is_deselected() {
    let script = "p 0 0 0 1 1 1 Material";
    let first = merge_proposal("X", 0.0, -0.06);
    let second = merge_proposal("X", 1.0, 0.90);

    let all_selected = simplify_proposals(script, &[first.clone(), second.clone()], 100.0, 9.5);
    assert_eq!(all_selected.applied_count, 2);
    assert!(all_selected.rejected.is_empty());
    assert!(all_selected.script.contains("-0.06"));
    assert!(all_selected.script.contains("0.9"));

    let later_only = simplify_proposals(script, &[second], 100.0, 9.5);
    assert_eq!(later_only.applied_count, 0);
    assert_eq!(later_only.rejected.len(), 1);
    assert_eq!(later_only.script, script);
}
