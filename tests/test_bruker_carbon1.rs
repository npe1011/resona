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
fn test_bruker_carbon1_directory_and_metadata() {
    let dir_path = get_test_data_path("Bruker_13C");
    let acqus_path = dir_path.join("acqus");

    // 1. ディレクトリ指定での読み込み
    let raw_from_dir = BrukerReader::read_fid(&dir_path).expect("Failed to read from Bruker 13C directory");

    // 2. acqus ファイル直接指定での読み込み
    let raw_from_file = BrukerReader::read_fid(&acqus_path).expect("Failed to read from Bruker acqus file");

    assert_eq!(raw_from_dir.data.len(), raw_from_file.data.len());
    assert_eq!(raw_from_dir.metadata.points, 32768);
    assert_eq!(raw_from_dir.metadata.nucleus, "13C");
    assert_eq!(raw_from_dir.metadata.solvent, "CDCl3");
    assert_eq!(raw_from_dir.metadata.scans, 128);
    assert_eq!(raw_from_dir.metadata.pulse_angle_deg, 30.0);
    assert_eq!(raw_from_dir.metadata.decoupling, "TRUE");
    assert_eq!(raw_from_dir.metadata.decoupling_nucleus, "1H");
    assert_eq!(raw_from_dir.metadata.decoupling_sequence, "waltz16");

    assert_relative_eq!(raw_from_dir.metadata.obs_freq_mhz, 125.77539271, epsilon = 1e-6);
    assert_relative_eq!(raw_from_dir.metadata.spectral_width_hz, 29761.9047619048, epsilon = 1e-5);
    assert_relative_eq!(raw_from_dir.metadata.center_ppm, 12574.71 / 125.762818, epsilon = 1e-5);
    assert_relative_eq!(raw_from_dir.metadata.relaxation_delay_sec, 2.0, epsilon = 1e-5);
    assert_eq!(raw_from_dir.metadata.pulse_width_us, Some(11.3));
    assert_relative_eq!(raw_from_dir.group_delay.unwrap(), 67.9838256835938, epsilon = 1e-6);
    assert_eq!(raw_from_dir.metadata.title, "PMP-vinyl-Indoline-sub-SI");
    assert_eq!(raw_from_file.metadata.title, "PMP-vinyl-Indoline-sub-SI");
    assert_eq!(raw_from_dir.metadata.probe, "");

    // 表示行の確認
    let rows = raw_from_dir.metadata.to_display_rows();
    assert!(rows.iter().any(|(k, v)| *k == "Title" && v == "PMP-vinyl-Indoline-sub-SI"));
    assert!(rows.iter().any(|(k, v)| *k == "Nucleus" && v == "13C"));
    assert!(rows.iter().any(|(k, v)| *k == "Decoupl." && v == "TRUE"));
    assert!(rows.iter().any(|(k, v)| *k == "Decoupl. Nuc." && v == "1H"));
    assert!(rows.iter().any(|(k, v)| *k == "Decoupl. Seq." && v == "waltz16"));
    assert!(!rows.iter().any(|(k, _)| *k == "Probe"));
}


#[test]
fn test_bruker_carbon1_pipeline_and_cdcl3_peak() {
    let dir_path = get_test_data_path("Bruker_13C");
    let raw_fid = BrukerReader::read_fid(&dir_path).expect("Failed to read Bruker 13C");

    let settings = FtSettings {
        window: WindowFunction::Exponential { lb: 1.0 },
        zf_factor: 1,
        remove_digital_filter: true,
    };

    let processed = process_raw_fid(&raw_fid, &settings, 0.0, 0.0).expect("Processing failed");

    let n = processed.ppm.len();
    assert_eq!(n, 32768);

    // CDCl3 ピーク (約 77.0 ppm) 付近の強度を検証
    // 76.0 〜 78.0 ppm の範囲内に極大が存在すること
    let mut max_in_cdcl3 = 0.0_f64;
    for i in 0..n {
        let ppm = processed.ppm[i];
        if ppm >= 76.0 && ppm <= 78.0 {
            let intensity = processed.complex_spectrum_unphased[n - 1 - i].norm();
            if intensity > max_in_cdcl3 {
                max_in_cdcl3 = intensity;
            }
        }
    }
    assert!(max_in_cdcl3 > 5e7, "CDCl3 peak should have high intensity, got {}", max_in_cdcl3);


    // Project load_bruker の検証
    let mut project = Project::new();
    project.load_bruker(&dir_path, Some(settings)).expect("Project load_bruker failed");
    assert!(project.ppm.is_some());
    assert!(project.spectrum_real.is_some());
    assert_eq!(project.metadata.nucleus, "13C");
}
