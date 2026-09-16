use heat3_povorotnik::corner::{create_corner, detect_constant_pair};

#[test]
fn detects_a_constant_axis_from_material_boxes_only() {
    // Both material boxes share Z range 0..0.1 while X and Y vary; the
    // boundary box spans a different Z range and must not spoil detection.
    let script = "p 0 0 0 0.5 0.4 0.1 Frame\np 0.5 0 0 1 0.6 0.1 Insulation\nb 0 0 0 5 1 1 2";
    let pair = detect_constant_pair(script).expect("pseudo-2D material section");
    assert_eq!(pair.axis, "Z");
    assert!((pair.min_val - 0.0).abs() < 1e-9);
    assert!((pair.max_val - 0.1).abs() < 1e-9);
}

#[test]
fn boundary_box_extent_does_not_define_corner_thickness() {
    let script = "p 0 0 0 0.1 0.4 1 Frame\np 0.1 0 0 0.2 0.4 1 Insulation\nb 0 0 0 10 0.4 1 2";
    let result = create_corner(script, "up");
    // The generated corner must be driven by the material section (0..0.2),
    // not by the 10 m boundary box.
    assert!(!result.contains(" 10 "));
}

#[test]
fn removes_fully_contained_duplicate_material_boxes() {
    // Two materials with identical geometry after trimming: one contains the
    // other, so the contained duplicate must not be emitted twice.
    let script = "p 0.15 0 0.1 0.27 0.42 0.57 Frame\np 0.15 0 0.1 0.27 0.42 0.52 Frame";
    let result = create_corner(script, "up");
    let frame_lines = result
        .lines()
        .filter(|line| line.starts_with("p ") && line.ends_with("Frame"))
        .count();
    assert!(
        frame_lines <= 2,
        "contained duplicate not removed: {result}"
    );
}
