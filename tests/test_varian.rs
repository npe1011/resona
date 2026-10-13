use approx::assert_relative_eq;
use resona::gui::app::actions::{detect_format, LoadedFormat};
use resona::{
    process_raw_fid, FtSettings, Project, VarianReader, WindowFunction,
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
fn test_varian_1h_directory_and_files() {
    let dir_path = get_test_data_path("Varian_1H.fid");
    let fid_path = dir_path.join("fid");
    let procpar_path = dir_path.join("procpar");

    // 1. フォーマット自動判定の検証
    assert_eq!(detect_format(&dir_path), Some(LoadedFormat::Varian));
    assert_eq!(detect_format(&fid_path), Some(LoadedFormat::Varian));
    assert_eq!(detect_format(&procpar_path), Some(LoadedFormat::Varian));

    // 2. ディレクトリ指定、fidファイル直接指定、procpar直接指定での読み込み検証
    let raw_from_dir = VarianReader::read_fid(&dir_path).expect("Failed to read from Varian directory");
    let raw_from_fid = VarianReader::read_fid(&fid_path).expect("Failed to read from Varian fid file");
    let raw_from_procpar = VarianReader::read_fid(&procpar_path).expect("Failed to read from Varian procpar file");

    assert_eq!(raw_from_dir.data.len(), 22321);
    assert_eq!(raw_from_fid.data.len(), 22321);
    assert_eq!(raw_from_procpar.data.len(), 22321);

    // メタデータの検証
    let meta = &raw_from_dir.metadata;
    assert_eq!(meta.nucleus, "1H");
    assert_eq!(meta.points, 22321);
    assert_eq!(meta.scans, 8);
    assert_eq!(meta.pulse_width_us, Some(9.75));
    assert_eq!(meta.digital_filter_delay, None);
    assert_eq!(raw_from_dir.group_delay, None);
    assert_relative_eq!(meta.obs_freq_mhz, 398.9244102, epsilon = 1e-4);
    assert_relative_eq!(meta.spectral_width_hz, 6377.55102041, epsilon = 1e-4);
    assert_relative_eq!(meta.temperature_celsius, 8.8, epsilon = 1e-4);
    assert_relative_eq!(meta.relaxation_delay_sec, 7.0, epsilon = 1e-4);
    assert_eq!(meta.solvent, "cdcl3");
    assert_eq!(meta.date_time, "2026-10-09 18:44:04");

    // 観測中心 PPM (rfp + (sw - 2*rfl) / (2*sfrq))
    assert!(meta.center_ppm.is_finite());

    // 表示用行リストの検証
    let rows = meta.to_display_rows();
    assert!(rows.iter().any(|(k, v)| *k == "Nucleus" && v == "1H"));
    assert!(rows.iter().any(|(k, v)| *k == "Scans" && v == "8"));
    assert!(rows.iter().any(|(k, v)| *k == "Temperature" && v == "8.8 C"));

    // 3. Project load_varian の検証
    let mut project = Project::new();
    project.load_varian(&dir_path, None).expect("Project load_varian failed");
    assert!(project.ppm.is_some());
    assert!(project.spectrum_real.is_some());
    assert_eq!(project.metadata.nucleus, "1H");
    assert!(project.max_intensity() > 0.0);

    // 化学シフトとピーク位置の検証 (CHCl3 ~7.26 ppm, TMS ~0.0 ppm)
    let ppm = project.ppm.as_ref().unwrap();
    let spec = project.spectrum_real.as_ref().unwrap();

    // 7.26 ppm 付近の極大
    let chcl3_idx = ppm
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (*a - 7.26).abs().partial_cmp(&(*b - 7.26).abs()).unwrap())
        .map(|(i, _)| i)
        .unwrap();
    assert!(spec[chcl3_idx] > 0.0, "CHCl3 peak at 7.26 ppm should be positive");

    // 0.0 ppm 付近 (TMS)
    let tms_idx = ppm
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (*a - 0.0).abs().partial_cmp(&(*b - 0.0).abs()).unwrap())
        .map(|(i, _)| i)
        .unwrap();
    assert!(spec[tms_idx] > 0.0, "TMS peak at 0.0 ppm should be positive");

    // 正しい向き (インデックス順で 7.26 ppm が 0.0 ppm より前 = 高ppmから低ppmへの単調減少)
    assert!(chcl3_idx < tms_idx, "7.26 ppm should appear before 0.0 ppm (monotonically decreasing)");
}

#[test]
fn test_varian_13c_directory() {
    let dir_path = get_test_data_path("Varian_13C.fid");

    assert_eq!(detect_format(&dir_path), Some(LoadedFormat::Varian));

    let raw = VarianReader::read_fid(&dir_path).expect("Failed to read Varian 13C");
    let meta = &raw.metadata;

    assert_eq!(meta.nucleus, "13C");
    assert_eq!(meta.points, 32768);
    assert_eq!(meta.scans, 192);
    assert_eq!(meta.decoupling, "TRUE");
    assert_eq!(meta.decoupling_nucleus, "1H");
    assert_relative_eq!(meta.obs_freq_mhz, 100.3200549, epsilon = 1e-4);
    assert_relative_eq!(meta.spectral_width_hz, 25000.0, epsilon = 1e-3);
    assert_eq!(raw.group_delay, None);

    let rows = meta.to_display_rows();
    assert!(rows.iter().any(|(k, v)| *k == "Nucleus" && v == "13C"));
    assert!(rows.iter().any(|(k, v)| *k == "Decoupl." && v == "TRUE"));
    assert!(rows.iter().any(|(k, v)| *k == "Decoupl. Nuc." && v == "1H"));

    let mut project = Project::new();
    project.load_varian(&dir_path, None).expect("Project load_varian failed for 13C");
    assert!(project.ppm.is_some());
    assert!(project.spectrum_real.is_some());
    assert_eq!(project.metadata.nucleus, "13C");
    assert!(project.max_intensity() > 0.0);

    // 13C CDCl3 トリプレット中心 (~77.06 ppm) の検証
    let ppm = project.ppm.as_ref().unwrap();
    let spec = project.spectrum_real.as_ref().unwrap();
    let cdcl3_idx = ppm
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (*a - 77.06).abs().partial_cmp(&(*b - 77.06).abs()).unwrap())
        .map(|(i, _)| i)
        .unwrap();
    assert!(spec[cdcl3_idx] > 0.0, "CDCl3 13C triplet should be positive");
}

#[test]
fn test_varian_1h_old_directory() {
    let dir_path = get_test_data_path("Varian_1H_old.fid");

    assert_eq!(detect_format(&dir_path), Some(LoadedFormat::Varian));

    let raw = VarianReader::read_fid(&dir_path).expect("Failed to read Varian 1H old");
    let meta = &raw.metadata;

    assert_eq!(meta.nucleus, "1H");
    assert_eq!(meta.points, 20000);
    assert_eq!(meta.scans, 72);
    assert_relative_eq!(meta.obs_freq_mhz, 400.115456786, epsilon = 1e-4);
    assert_relative_eq!(meta.spectral_width_hz, 4997.50124938, epsilon = 1e-2);
    assert_eq!(raw.group_delay, None);

    let mut project = Project::new();
    project.load_varian(&dir_path, None).expect("Project load_varian failed for 1H old");
    assert!(project.ppm.is_some());
    assert!(project.spectrum_real.is_some());
    assert_eq!(project.metadata.nucleus, "1H");
    assert!(project.max_intensity() > 0.0);

    // DMSO (~2.50 ppm) および H2O (~3.31 ppm) の検証
    let ppm = project.ppm.as_ref().unwrap();
    let spec = project.spectrum_real.as_ref().unwrap();

    let h2o_idx = ppm
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (*a - 3.31).abs().partial_cmp(&(*b - 3.31).abs()).unwrap())
        .map(|(i, _)| i)
        .unwrap();
    let dmso_idx = ppm
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (*a - 2.50).abs().partial_cmp(&(*b - 2.50).abs()).unwrap())
        .map(|(i, _)| i)
        .unwrap();

    assert!(spec[h2o_idx] > 0.0, "H2O peak should be positive");
    assert!(spec[dmso_idx] > 0.0, "DMSO peak should be positive");
    assert!(h2o_idx < dmso_idx, "3.31 ppm should appear before 2.50 ppm (monotonically decreasing)");
}

#[test]
fn test_varian_fft_processing() {
    let dir_path = get_test_data_path("Varian_1H.fid");
    let raw_fid = VarianReader::read_fid(&dir_path).expect("Failed to read Varian 1H");

    let settings = FtSettings {
        window: WindowFunction::Exponential { lb: 0.5 },
        zf_factor: 2,
        remove_digital_filter: false, // Varian にはデジタルフィルタ遅延なし
    };

    let processed = process_raw_fid(&raw_fid, &settings, 0.0, 0.0).expect("Processing failed");

    let max_val = processed.spectrum_real.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    assert!(max_val > 0.0);
}

#[test]
fn test_varian_1h_old_full_auto() {
    let dir_path = get_test_data_path("Varian_1H_old.fid");
    let mut project = Project::new();
    project.load_varian(&dir_path, None).expect("load_varian failed");
    assert_eq!(project.metadata.solvent, "DMSO");
    assert_eq!(project.metadata.nucleus, "1H");

    // 溶媒選択解決の検証
    let (solvent_idx, target_ppm) = resona::core::analysis::resolve_solvent_selection(
        &project.metadata.solvent,
        &project.metadata.nucleus,
    );
    assert_eq!(target_ppm, 2.50);
    let solvents = resona::core::analysis::get_available_solvents(&project.metadata.nucleus);
    assert_eq!(solvents[solvent_idx].0.name, "DMSO-d6");

    // auto_detect_reference_peak の検証
    let spec = project.spectrum_real.as_ref().unwrap();
    let ppm = project.ppm.as_ref().unwrap();
    let detected = resona::core::analysis::auto_detect_reference_peak(spec, ppm, 2.50, 0.20, 2.0);
    assert!(detected.is_some(), "DMSO peak should be detected within ±0.20 ppm");
    let peak_ppm = detected.unwrap();
    assert!((peak_ppm - 2.483).abs() < 0.01, "Detected peak should be near 2.483 ppm, got {}", peak_ppm);

    // Full auto 実行の検証
    let report = project.execute_full_auto(resona::BaselineMethod::None, false);
    assert!(report.reference_applied, "Reference should be applied! Desc: {}", report.reference_desc);
    assert!(report.reference_desc.contains("DMSO-d6"));

    // シフト後、DMSO ピークが 2.50 ppm に合致していることを検証
    let ppm_after = project.ppm.as_ref().unwrap();
    let spec_after = project.spectrum_real.as_ref().unwrap();
    let dmso_idx = ppm_after
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (*a - 2.50).abs().partial_cmp(&(*b - 2.50).abs()).unwrap())
        .map(|(i, _)| i)
        .unwrap();
    assert!(spec_after[dmso_idx] > 0.0);
    assert!((ppm_after[dmso_idx] - 2.50).abs() < 0.005);
}


