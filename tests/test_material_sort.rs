use heat3_povorotnik::material_sort::is_material_reorder_safe;
use heat3_povorotnik::material_sort::materials_by_normalized_name_checked;
use heat3_povorotnik::material_sort::{
    extract_material_entries, parse_mtl_file, sort_material_boxes_by_order,
    sort_material_names_by_conductivity, write_mtl_file, MtlMaterial, NAME_FIELD_SIZE,
    NUMERIC_FIELD_SIZE, RECORD_SIZE,
};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn test_extract_material_entries_preserves_first_seen_order() {
    let script = "p\t0\t0\t0\t1\t1\t1\tBeta\t! material box\r\nb\t0\t0\t0\t1\t1\t1\t2\t! BC box\r\np\t0\t0\t0\t1\t1\t1\tAlpha\t! material box\r\np\t0\t0\t0\t1\t1\t1\tBeta\t! material box";
    let entries = extract_material_entries(script);
    let pairs: Vec<(&str, i32)> = entries.iter().map(|e| (e.name.as_str(), e.count)).collect();
    assert_eq!(pairs, vec![("Beta", 2), ("Alpha", 1)]);
}

#[test]
fn test_extract_material_entries_accepts_commentless_and_commented_p_commands() {
    let script = "p 0 0 0 1 1 1 Mineral wool\r\np 0 0 0 1 1 1 Brick ! hand-written note\r\nb 0 0 0 1 1 1 2 Mineral wool";

    let entries = extract_material_entries(script);

    let pairs: Vec<(&str, i32)> = entries.iter().map(|e| (e.name.as_str(), e.count)).collect();
    assert_eq!(pairs, vec![("Mineral wool", 1), ("Brick", 1)]);
}

#[test]
fn test_sort_material_boxes_reorders_only_material_box_lines() {
    let script = "! header\r\np\t0\t0\t0\t1\t1\t1\tBeta\t! material box\r\nb\t0\t0\t0\t1\t1\t1\t2\t! BC box\r\np\t0\t0\t0\t1\t1\t1\tAlpha\t! material box\r\ne\t0\t0\t0\t1\t1\t1\tignored";
    let result = sort_material_boxes_by_order(script, &["Alpha".to_string(), "Beta".to_string()]);
    let lines: Vec<&str> = result.lines().collect();
    assert_eq!(lines[0], "! header");
    assert!(lines[1].ends_with("\tAlpha\t! material box"));
    assert!(lines[2].ends_with("\t! BC box"));
    assert!(lines[3].ends_with("\tBeta\t! material box"));
    assert!(lines[4].starts_with("e\t"));
    assert_eq!(result.matches('\t').count(), script.matches('\t').count());
    assert_eq!(
        result.matches("\r\n").count(),
        script.matches("\r\n").count()
    );
}

#[test]
fn test_sort_material_boxes_reorders_commentless_p_commands() {
    let script = "p 0 0 0 1 1 1 Beta\np 0 0 0 1 1 1 Alpha ! imported manually";

    let result = sort_material_boxes_by_order(script, &["Alpha".to_string(), "Beta".to_string()]);

    assert_eq!(
        result,
        "p 0 0 0 1 1 1 Alpha ! imported manually\np 0 0 0 1 1 1 Beta"
    );
}

#[test]
fn test_parse_mtl_file_reads_windows_1251_materials() {
    let dir = std::env::temp_dir();
    let path = dir.join("test_materials.mtl");

    write_mtl_file(
        &path,
        &[
            MtlMaterial {
                name: "Бетон".to_string(),
                thermal_x: 0.56,
                thermal_y: 0.57,
                volume_heat: 1000.0,
                rgb_r: 1,
                rgb_g: 2,
                rgb_b: 3,
                special_value: 4,
            },
            MtlMaterial {
                name: "Alpha".to_string(),
                thermal_x: 0.04,
                thermal_y: 0.0,
                volume_heat: 0.0,
                rgb_r: 1,
                rgb_g: 2,
                rgb_b: 3,
                special_value: 4,
            },
        ],
    )
    .unwrap();

    let materials = parse_mtl_file(&path, false).unwrap();
    std::fs::remove_file(&path).ok();

    assert_eq!(materials.len(), 2);
    assert_eq!(materials[0].name, "Бетон");
    assert!((materials[0].thermal_x - 0.56).abs() < 1e-9);
    assert!((materials[0].thermal_y - 0.57).abs() < 1e-9);
    assert!((materials[0].volume_heat - 1000.0).abs() < 1e-9);
    assert_eq!(materials[0].rgb_r, 1);
    assert_eq!(materials[0].rgb_g, 2);
    assert_eq!(materials[0].rgb_b, 3);
    assert_eq!(materials[0].special_value, 4);

    assert_eq!(materials[1].name, "Alpha");
    assert!((materials[1].thermal_x - 0.04).abs() < 1e-9);
}

#[test]
fn test_parse_mtl_file_rejects_non_finite_numeric_fields() {
    let path = std::env::temp_dir().join("test_materials_non_finite.mtl");
    let mut record = vec![0; RECORD_SIZE];
    record[0] = 7;
    record[1..8].copy_from_slice(b"Invalid");
    let thermal_x_offset = 1 + NAME_FIELD_SIZE;
    record[thermal_x_offset] = 3;
    record[thermal_x_offset + 1..thermal_x_offset + 4].copy_from_slice(b"NaN");
    let thermal_y_offset = thermal_x_offset + 1 + NUMERIC_FIELD_SIZE;
    record[thermal_y_offset] = 3;
    record[thermal_y_offset + 1..thermal_y_offset + 4].copy_from_slice(b"inf");
    let volume_heat_offset = thermal_y_offset + 1 + NUMERIC_FIELD_SIZE;
    record[volume_heat_offset] = 4;
    record[volume_heat_offset + 1..volume_heat_offset + 5].copy_from_slice(b"-inf");
    std::fs::write(&path, record).unwrap();

    let error = parse_mtl_file(&path, true).unwrap_err();
    std::fs::remove_file(&path).ok();
    assert!(error.contains("non-finite"));
}

#[test]
fn test_sort_material_names_by_thermal_conductivity_unknown_last() {
    let mut map = HashMap::new();
    map.insert(
        "alpha".to_string(),
        MtlMaterial {
            name: "Alpha".to_string(),
            thermal_x: 0.04,
            thermal_y: 0.04,
            volume_heat: 0.0,
            rgb_r: 0,
            rgb_g: 0,
            rgb_b: 0,
            special_value: 0,
        },
    );
    map.insert(
        "beta".to_string(),
        MtlMaterial {
            name: "Beta".to_string(),
            thermal_x: 0.20,
            thermal_y: 0.20,
            volume_heat: 0.0,
            rgb_r: 0,
            rgb_g: 0,
            rgb_b: 0,
            special_value: 0,
        },
    );
    let names = vec![
        "Unknown".to_string(),
        "Beta".to_string(),
        "Alpha".to_string(),
    ];
    let result = sort_material_names_by_conductivity(&names, &map);
    assert_eq!(result, vec!["Alpha", "Beta", "Unknown"]);
}

#[test]
fn test_write_mtl_file_replaces_existing_file_atomically() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("heat3-mtl-{unique}"));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("materials.mtl");

    std::fs::write(&path, b"stale").unwrap();
    write_mtl_file(
        &path,
        &[MtlMaterial {
            name: "Gamma".to_string(),
            thermal_x: 0.15,
            thermal_y: 0.15,
            volume_heat: 100.0,
            rgb_r: 5,
            rgb_g: 6,
            rgb_b: 7,
            special_value: 8,
        }],
    )
    .unwrap();

    let materials = parse_mtl_file(&path, true).unwrap();
    assert_eq!(materials.len(), 1);
    assert_eq!(materials[0].name, "Gamma");

    let leftovers = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.ends_with(".tmp"))
        .collect::<Vec<_>>();
    assert!(
        leftovers.is_empty(),
        "temporary files leaked: {leftovers:?}"
    );

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_strict_mtl_load_rejects_corrupt_records_and_trailing_bytes() {
    let path = std::env::temp_dir().join("test_materials_corrupt.mtl");
    let material = MtlMaterial {
        name: "Valid".to_string(),
        thermal_x: 0.15,
        thermal_y: 0.15,
        volume_heat: 100.0,
        rgb_r: 5,
        rgb_g: 6,
        rgb_b: 7,
        special_value: 8,
    };
    write_mtl_file(&path, &[material.clone(), material.clone(), material]).unwrap();

    let mut corrupt_middle = std::fs::read(&path).unwrap();
    corrupt_middle[RECORD_SIZE] = 255;
    std::fs::write(&path, &corrupt_middle).unwrap();
    assert!(parse_mtl_file(&path, true).is_err());
    assert_eq!(parse_mtl_file(&path, false).unwrap().len(), 2);

    let mut trailing = std::fs::read(&path).unwrap();
    trailing.push(0);
    std::fs::write(&path, trailing).unwrap();
    assert!(parse_mtl_file(&path, true).is_err());

    std::fs::remove_file(&path).ok();
}

fn material(name: &str, thermal_x: f64) -> MtlMaterial {
    MtlMaterial {
        name: name.to_string(),
        thermal_x,
        thermal_y: thermal_x,
        volume_heat: 0.0,
        rgb_r: 0,
        rgb_g: 0,
        rgb_b: 0,
        special_value: 0,
    }
}

#[test]
fn test_material_index_rejects_conflicting_duplicate_names() {
    let conflicting = vec![material("Brick", 0.1), material("brick ", 0.2)];
    assert!(materials_by_normalized_name_checked(&conflicting).is_err());

    let identical = vec![material("Brick", 0.1), material("brick", 0.1)];
    let map = materials_by_normalized_name_checked(&identical).unwrap();
    assert_eq!(map.len(), 1);
}

#[test]
fn test_reorder_safety_allows_disjoint_and_blocks_overlapping_boxes() {
    let disjoint = "p 0 0 0 1 1 1 A\np 2 0 0 3 1 1 B";
    assert!(is_material_reorder_safe(disjoint));

    let overlapping = "p 0 0 0 1 1 1 A\np 0.5 0 0 1.5 1 1 B";
    assert!(!is_material_reorder_safe(overlapping));
}

#[test]
fn test_reorder_safety_blocks_material_box_crossing_an_empty_box() {
    let script = "p 0 0 0 1 1 1 A\ne 0.5 0 0 1.5 1 1\np 2 0 0 3 1 1 B";
    assert!(!is_material_reorder_safe(script));
}
