use heat3_povorotnik::turner2d::{
    parse_2d_script, serialize_2d_script, transform_2d_script, unsupported_geometry_commands,
    TRANSFORMS_2D,
};

#[test]
fn test_unsupported_2d_spatial_commands_are_reported() {
    let script = "r 0 0 1 1 Brick\ns 1 0 0.2 1 Insulation\nx 0.5 0.5";
    let unsupported = unsupported_geometry_commands(script);
    assert_eq!(unsupported, vec!["s".to_owned(), "x".to_owned()]);

    assert!(unsupported_geometry_commands("R 0 0 1 1 Brick").is_empty());
}

#[test]
fn test_parse_and_serialize_2d_script_outputs_dot_decimal_separator() {
    let script = "r 0 0,43 0,2 0,69 acrylic resin, no cap., CEN\r\n acrylic resin, no cap., CEN ";
    let result = serialize_2d_script(&parse_2d_script(script));
    assert_eq!(
        result,
        "r 0 0.43 0.2 0.69 acrylic resin, no cap., CEN\r\n acrylic resin, no cap., CEN "
    );
}

#[test]
fn test_transform_2d_rotate_clockwise() {
    let script = "r 0 0,43 0,2 0,69 acrylic resin, no cap., CEN\r\n acrylic resin, no cap., CEN ";
    let result = transform_2d_script(script, "rotate_clockwise").unwrap();
    let first = result.lines().next().unwrap();
    assert_eq!(first, "r 0.43 -0.2 0.69 0 acrylic resin, no cap., CEN");
    assert_eq!(
        result.lines().nth(1).unwrap(),
        " acrylic resin, no cap., CEN "
    );
    assert_eq!(
        result.matches("\r\n").count(),
        script.matches("\r\n").count()
    );
}

#[test]
fn test_transform_2d_accepts_and_preserves_uppercase_rectangle_commands() {
    let script = "R 0.1 0.1 0.5 0.2 concrete, IEA\nr 0.3 0.4 0.7 0.2\ns 0 0 1 1";

    let result = transform_2d_script(script, "rotate_clockwise").unwrap();

    assert_eq!(
        result,
        "R 0.1 -0.5 0.2 -0.1 concrete, IEA\nr 0.2 -0.7 0.4 -0.3\ns 0 0 1 1"
    );
}

#[test]
fn test_transform_2d_rotate_counterclockwise() {
    let script = "r 0 0,43 0,2 0,69 acrylic resin, no cap., CEN\r\n acrylic resin, no cap., CEN ";
    let result = transform_2d_script(script, "rotate_counterclockwise").unwrap();
    let first = result.lines().next().unwrap();
    assert_eq!(first, "r -0.69 0 -0.43 0.2 acrylic resin, no cap., CEN");
}

#[test]
fn test_transform_2d_mirror_x() {
    let script = "r 0 0,43 0,2 0,69 acrylic resin, no cap., CEN\r\n acrylic resin, no cap., CEN ";
    let result = transform_2d_script(script, "mirror_x").unwrap();
    let first = result.lines().next().unwrap();
    assert_eq!(first, "r 0 -0.69 0.2 -0.43 acrylic resin, no cap., CEN");
}

#[test]
fn test_transform_2d_mirror_y() {
    let script = "r 0 0,43 0,2 0,69 acrylic resin, no cap., CEN\r\n acrylic resin, no cap., CEN ";
    let result = transform_2d_script(script, "mirror_y").unwrap();
    let first = result.lines().next().unwrap();
    assert_eq!(first, "r -0.2 0.43 0 0.69 acrylic resin, no cap., CEN");
}

#[test]
fn test_transform_2d_preserves_line_count_and_separators() {
    let script = "\nr\t0\t0,43\t0,2\t0,69\tacrylic resin, no cap., CEN\n# comment\nr 0,14 0,46 0,58 0,63 acrylic resin, no cap., CEN";
    for (name, _) in TRANSFORMS_2D {
        let result = transform_2d_script(script, name).unwrap();
        assert_eq!(
            result.lines().count(),
            script.lines().count(),
            "transform: {name}"
        );
        assert_eq!(result.lines().next().unwrap(), "", "transform: {name}");
        assert_eq!(
            result.lines().nth(2).unwrap(),
            "# comment",
            "transform: {name}"
        );
        assert_eq!(
            result.matches('\t').count(),
            script.matches('\t').count(),
            "transform: {name}"
        );
        assert_eq!(
            result.matches('\n').count(),
            script.matches('\n').count(),
            "transform: {name}"
        );
    }
}
