use heat3_povorotnik::parser::{parse_script, serialize_script};
use heat3_povorotnik::transforms::{
    apply_transform, transform_enable_flags, unsupported_geometry_commands, ENABLE_TRANSFORMS,
};

#[test]
fn test_unsupported_3d_geometry_commands_are_reported() {
    let script = "p 0 0 0 1 1 1 Brick\ns 1 0 0 0.2 1 1 Insulation\nh 0.2 0.2 0.2 0.3 0.3 0.3 100";
    let unsupported = unsupported_geometry_commands(script);
    assert_eq!(unsupported, vec!["s".to_owned(), "h".to_owned()]);

    assert!(unsupported_geometry_commands("p 0 0 0 1 1 1 Brick\nb 0 0 0 1 1 1 2").is_empty());
}

#[test]
fn test_enable_flag_permutation_order() {
    let cases = [
        ("rotate_clockwise", "231045"),
        ("rotate_counterclockwise", "320145"),
        ("swap_xy_xz", "014523"),
        ("mirror_xy_x", "013245"),
        ("mirror_xy_y", "102345"),
        ("mirror_xz_x", "012354"),
        ("mirror_xz_z", "102345"),
    ];
    for (name, expected) in &cases {
        let f = ENABLE_TRANSFORMS
            .iter()
            .find(|(n, _)| *n == *name)
            .unwrap()
            .1;
        assert_eq!(f("012345"), expected.to_string(), "transform: {name}");
    }
}

#[test]
fn test_enable_flag_transforms_bc_box_mask() {
    let cases = [
        ("rotate_clockwise", "011111"),
        ("rotate_counterclockwise", "101111"),
        ("swap_xy_xz", "111101"),
        ("mirror_xy_x", "111011"),
        ("mirror_xy_y", "110111"),
        ("mirror_xz_x", "110111"),
        ("mirror_xz_z", "110111"),
    ];
    for (name, expected) in &cases {
        let result = transform_enable_flags("%enable=110111 ! BC box", name);
        assert_eq!(
            result,
            format!("%enable={expected} ! BC box"),
            "transform: {name}"
        );
    }
}

#[test]
fn test_enable_flag_ignores_non_six_bit_masks() {
    for text in &[
        "%enable=1101110 ! BC box",
        "%enable=11011 ! BC box",
        "%enable=11011x ! BC box",
    ] {
        assert_eq!(transform_enable_flags(text, "swap_xy_xz"), text.to_string());
    }
}

#[test]
fn test_transform_pipeline_updates_bc_box_enable_mask() {
    let sample = "p\t0\t0\t0\t0.59\t0.33\t0.1\tУплотнитель EPDM\t! material box\nb\t-0.09\t0.3\t-0.05\t-0.04\t0.37\t0.05\t2\t%enable=110111\t! BC box";
    let mut lines = parse_script(sample);
    let transform_name = "swap_xy_xz";
    for line in &mut lines {
        if let Some(seg) = &line.segment {
            if let Some(new_seg) = apply_transform(transform_name, seg) {
                line.segment = Some(new_seg);
            }
        }
        if line.label.as_deref() == Some("b") {
            line.trailing = transform_enable_flags(&line.trailing, transform_name);
        }
    }
    let result = serialize_script(&lines, "\r\n");
    assert!(result.contains("%enable=111101"));
    assert!(result.contains("! BC box"));
}
