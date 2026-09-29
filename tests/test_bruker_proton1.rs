use approx::assert_relative_eq;
use resona::{
    process_raw_fid, BrukerReader, FtSettings, Project, WindowFunction,
};


fn get_test_data_path(name: &str) -> std::path::PathBuf {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("test_data").join(name);
    if path.exists() {
        return path;
    }
    let alt = manifest_dir.join("../../test_data").join(name);
    if alt.exists() {
        return alt;
    }
    path
}

#[test]
fn test_bruker_proton1_directory_and_file_read() {
    let dir_path = get_test_data_path("Bruker_1H");
    let fid_path = dir_path.join("fid");

    // 1. ディレクトリ指定での読み込み
    let raw_from_dir = BrukerReader::read_fid(&dir_path).expect("Failed to read from Bruker directory");

    // 2. fid ファイル直接指定での読み込み
    let raw_from_file = BrukerReader::read_fid(&fid_path).expect("Failed to read from Bruker fid file");

    assert_eq!(raw_from_dir.data.len(), raw_from_file.data.len());
    assert_eq!(raw_from_dir.metadata.nucleus, raw_from_file.metadata.nucleus);
    assert_eq!(raw_from_dir.metadata.points, 32768);
    assert_eq!(raw_from_dir.metadata.nucleus, "1H");
    assert_eq!(raw_from_dir.metadata.solvent, "CDCl3");
    assert_eq!(raw_from_dir.metadata.scans, 16);
    assert_eq!(raw_from_dir.metadata.pulse_angle_deg, 30.0);

    assert_relative_eq!(raw_from_dir.metadata.obs_freq_mhz, 500.153088426, epsilon = 1e-6);
    assert_relative_eq!(raw_from_dir.metadata.spectral_width_hz, 10000.0, epsilon = 1e-6);
    assert_relative_eq!(raw_from_dir.metadata.center_ppm, 3088.426 / 500.15, epsilon = 1e-5);
    assert_relative_eq!(raw_from_dir.metadata.temperature_celsius, 298.1454 - 273.15, epsilon = 1e-4);
    assert_relative_eq!(raw_from_dir.metadata.relaxation_delay_sec, 1.0, epsilon = 1e-5);
    assert_eq!(raw_from_dir.metadata.pulse_width_us, Some(7.1));
    assert_eq!(raw_from_dir.group_delay, Some(76.0));
    assert_eq!(raw_from_dir.metadata.title, "PMP-vinyl-Indoline-sub-SI");
    assert_eq!(raw_from_file.metadata.title, "PMP-vinyl-Indoline-sub-SI");
    assert_eq!(raw_from_dir.metadata.probe, "");

    // メタデータ行表示の確認
    let rows = raw_from_dir.metadata.to_display_rows();
    assert!(rows.iter().any(|(k, v)| *k == "Title" && v == "PMP-vinyl-Indoline-sub-SI"));
    assert!(rows.iter().any(|(k, v)| *k == "Nucleus" && v == "1H"));
    assert!(rows.iter().any(|(k, v)| *k == "Solvent" && v == "CDCl3"));
    assert!(rows.iter().any(|(k, v)| *k == "Scans" && v == "16"));
    assert!(!rows.iter().any(|(k, _)| *k == "Probe"));
}


#[test]
fn test_bruker_proton1_pipeline_and_project() {
    let dir_path = get_test_data_path("Bruker_1H");
    let raw_fid = BrukerReader::read_fid(&dir_path).expect("Failed to read Bruker 1H");

    let settings = FtSettings {
        window: WindowFunction::Exponential { lb: 0.3 },
        zf_factor: 2,
        remove_digital_filter: true,
    };

    let processed = process_raw_fid(&raw_fid, &settings, 0.0, 0.0).expect("Processing failed");

    let n = processed.ppm.len();
    assert_eq!(n, 65536);
    assert_eq!(processed.spectrum_real.len(), 65536);

    // 最大強度が正のピーク（吸収形）になっていること
    let max_val = processed.spectrum_real.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let min_val = processed.spectrum_real.iter().cloned().fold(f64::INFINITY, f64::min);
    assert!(max_val > 1e8);
    assert!(max_val > min_val.abs());

    // Project による load_bruker の検証
    let mut project = Project::new();
    project.load_bruker(&dir_path, Some(settings)).expect("Project load_bruker failed");
    assert!(project.ppm.is_some());
    assert!(project.spectrum_real.is_some());
    assert_eq!(project.metadata.nucleus, "1H");
    assert_eq!(project.metadata.solvent, "CDCl3");
}
