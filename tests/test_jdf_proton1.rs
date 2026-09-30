use approx::assert_relative_eq;
use resona::{process_raw_fid, FtSettings, JeolJdfReader, NmrDataSource, WindowFunction};

fn get_test_data_path(file_name: &str) -> std::path::PathBuf {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("test_data").join(file_name);
    if path.exists() {
        return path;
    }
    let alt = manifest_dir.join("../../test_data").join(file_name);
    if alt.exists() {
        return alt;
    }
    path
}

#[test]
fn test_proton1_pipeline() {
    let path = get_test_data_path("Proton1.jdf");
    let raw_fid = JeolJdfReader::read_fid(&path).expect("Failed to read Proton1.jdf");

    assert_eq!(raw_fid.metadata.points, 16384);
    assert_eq!(raw_fid.metadata.nucleus, "1H");
    assert_relative_eq!(raw_fid.metadata.obs_freq_mhz, 594.1705816769363, epsilon = 1e-6);
    assert_relative_eq!(raw_fid.metadata.spectral_width_hz, 11140.819964349375, epsilon = 1e-6);
    assert_relative_eq!(raw_fid.metadata.center_ppm, 5.0, epsilon = 1e-6);
    assert_eq!(raw_fid.group_delay, Some(19.6875));
    assert_eq!(raw_fid.metadata.instrument, "JEOL unknown");
    assert_eq!(raw_fid.metadata.probe, "");



    let settings = FtSettings {
        window: WindowFunction::Exponential { lb: 0.12 },
        zf_factor: 4,
        remove_digital_filter: true,
    };

    let p0 = 35.0;
    let p1 = -15.0;
    let processed = process_raw_fid(&raw_fid, &settings, p0, p1).expect("Processing failed");

    let n = processed.ppm.len();
    assert_eq!(n, 65536);
    assert_eq!(processed.spectrum_real.len(), 65536);

    // PPM軸の検証 (Python版: 14.375102292094702 .. -4.374816186678075)
    assert_relative_eq!(processed.ppm[0], 14.375102292094702, epsilon = 1e-8);
    assert_relative_eq!(processed.ppm[n - 1], -4.374816186678075, epsilon = 1e-8);

    let sum: f64 = processed.spectrum_real.iter().sum();
    let min = processed.spectrum_real.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = processed.spectrum_real.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    println!("Proton1 real[0..5]: {:?}", &processed.spectrum_real.as_slice().unwrap()[0..5]);
    println!("sum: {:.4}, min: {:.4}, max: {:.4}", sum, min, max);

    // スペクトル先頭の点検証 (末尾回り込み除去後の正常値)
    assert_relative_eq!(processed.spectrum_real[0], 16.612834679054703, max_relative = 1e-5);

    // 全体の総和・極値の検証
    assert_relative_eq!(sum, -8874394.396638874, max_relative = 1e-3);
    assert_relative_eq!(min, -578901.7778628239, max_relative = 1e-4);
    assert_relative_eq!(max, 118954.49276395496, max_relative = 1e-4);
}
