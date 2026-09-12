use heat3_povorotnik::air_cavities::{
    build_air_cavity_materials, format_air_cavity_name, parse_air_cavities_info_log,
    upsert_air_cavity_materials, DEFAULT_AIR_CAVITY_NAME_MASK,
};
use heat3_povorotnik::material_sort::{parse_mtl_file, write_mtl_file, MtlMaterial};

const SAMPLE_LOG: &str = "
Preparing conductances...
--------- FRAME CAVITIES ---------
Number of frame cavities: 2
Cavity  b [mm]  d [mm]  area [mm²]
 1      130     40      5200
 2      40      50      2000

Cavity  Tmax    Tmin    ha      hr      lambda  Iter:1
 1      0       0       0.1923  2.1716  0.3073
 2      0       0       0.625   2.7951  0.1368

Cavity  Tmax    Tmin    ha      hr      lambda  Iter:3
 1      0.5442  0.3621  0.4137  2.1825  0.3375
 2      0.8584  0.7947  0.625   2.8206  0.1378
";

#[test]
fn test_parse_air_cavities_info_log_uses_last_lambda_table() {
    let cavities = parse_air_cavities_info_log(SAMPLE_LOG).unwrap();
    assert_eq!(cavities.len(), 2);
    assert_eq!(cavities[0].number, 1);
    assert!((cavities[0].b_mm - 130.0).abs() < 1e-9);
    assert!((cavities[0].d_mm - 40.0).abs() < 1e-9);
    assert!((cavities[0].area_mm2 - 5200.0).abs() < 1e-9);
    assert!((cavities[0].lambda_value - 0.3375).abs() < 1e-9);
    assert_eq!(cavities[0].iteration, 3);
    assert_eq!(cavities[1].number, 2);
    assert!((cavities[1].lambda_value - 0.1378).abs() < 1e-9);
}

#[test]
fn test_parse_air_cavities_info_log_rejects_missing_matching_dimensions() {
    let log = "Cavity  b [mm]  d [mm]  area [mm²]\n 1      130     40      5200\n\nCavity  Tmax    Tmin    ha      hr      lambda  Iter:1\n 2      0       0       0.625   2.7951  0.1368";
    let result = parse_air_cavities_info_log(log);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("нет"));
}

#[test]
fn test_parse_air_cavities_info_log_rejects_non_finite_values() {
    for value in ["NaN", "inf", "-inf"] {
        let log = SAMPLE_LOG.replacen("130", value, 1);
        assert!(
            parse_air_cavities_info_log(&log).is_err(),
            "expected {value} to be rejected"
        );
    }
}

#[test]
fn test_build_air_cavity_materials_uses_mask_color_and_special_value() {
    let cavities = parse_air_cavities_info_log(SAMPLE_LOG).unwrap();
    let specs = build_air_cavity_materials(
        &cavities,
        "Air [номер] [ширина]x[глубина] L[лямбда]",
        (10, 20, 30),
        7,
    )
    .unwrap();
    assert_eq!(specs.len(), 2);
    assert_eq!(specs[0].material.name, "Air 1 130x40 L0.3375");
    assert!((specs[0].material.thermal_x - 0.3375).abs() < 1e-9);
    assert!((specs[0].material.thermal_y - 0.3375).abs() < 1e-9);
    assert!((specs[0].material.volume_heat - 0.0).abs() < 1e-9);
    assert_eq!(specs[0].material.rgb_r, 10);
    assert_eq!(specs[0].material.rgb_g, 20);
    assert_eq!(specs[0].material.rgb_b, 30);
    assert_eq!(specs[0].material.special_value, 7);
}

#[test]
fn test_build_air_cavity_materials_rejects_unknown_simple_token() {
    let cavities = parse_air_cavities_info_log(SAMPLE_LOG).unwrap();
    let result = build_air_cavity_materials(&cavities, "Air [неизвестно]", (10, 20, 30), 7);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Неизвестный маркер"));
}

#[test]
fn test_format_air_cavity_name_supports_escaped_braces() {
    let cavities = parse_air_cavities_info_log(SAMPLE_LOG).unwrap();
    let name = format_air_cavity_name("Air {{literal}} {n:03d}", &cavities[0]).unwrap();
    assert_eq!(name, "Air {literal} 001");
}

#[test]
fn test_default_air_cavity_name_mask_is_readable_utf8() {
    let cavities = parse_air_cavities_info_log(SAMPLE_LOG).unwrap();
    let name = format_air_cavity_name(DEFAULT_AIR_CAVITY_NAME_MASK, &cavities[0]).unwrap();
    assert_eq!(name, "Прослойка 001");
}

#[test]
fn test_format_air_cavity_name_rejects_unknown_field() {
    let cavities = parse_air_cavities_info_log(SAMPLE_LOG).unwrap();
    let result = format_air_cavity_name("Air {unknown}", &cavities[0]);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Неизвестное поле"));
}

#[test]
fn test_build_air_cavity_materials_rejects_duplicate_generated_names() {
    let cavities = parse_air_cavities_info_log(SAMPLE_LOG).unwrap();
    let result = build_air_cavity_materials(&cavities, "Air", (10, 20, 30), 7);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("дублиру"));
}

#[test]
fn test_upsert_air_cavity_materials_updates_existing_and_appends_new() {
    let dir = std::env::temp_dir();
    let path = dir.join("test_air_cavity_materials.mtl");

    write_mtl_file(
        &path,
        &[
            MtlMaterial {
                name: "Existing".to_string(),
                thermal_x: 1.0,
                thermal_y: 2.0,
                volume_heat: 3.0,
                rgb_r: 1,
                rgb_g: 2,
                rgb_b: 3,
                special_value: 4,
            },
            MtlMaterial {
                name: "Air 1".to_string(),
                thermal_x: 9.0,
                thermal_y: 9.0,
                volume_heat: 9.0,
                rgb_r: 9,
                rgb_g: 9,
                rgb_b: 9,
                special_value: 9,
            },
        ],
    )
    .unwrap();

    let cavities = parse_air_cavities_info_log(SAMPLE_LOG).unwrap();
    let specs = build_air_cavity_materials(&cavities, "Air [номер]", (10, 20, 30), 7).unwrap();
    let result = upsert_air_cavity_materials(&path, &specs).unwrap();

    assert_eq!(result.updated_names, vec!["Air 1"]);
    assert_eq!(result.added_names, vec!["Air 2"]);

    let materials = parse_mtl_file(&path, true).unwrap();
    std::fs::remove_file(&path).ok();

    assert_eq!(materials.len(), 3);
    assert_eq!(materials[0].name, "Existing");
    assert_eq!(materials[1].name, "Air 1");
    assert!((materials[1].thermal_x - 0.3375).abs() < 1e-9);
    assert_eq!(materials[2].name, "Air 2");
    assert!((materials[2].thermal_x - 0.1378).abs() < 1e-9);
}
