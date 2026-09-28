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
fn test_carbon1_pipeline() {
    let path = get_test_data_path("Carbon1.jdf");
    let raw_fid = JeolJdfReader::read_fid(&path).expect("Failed to read Carbon1.jdf");

    assert_eq!(raw_fid.metadata.points, 32768);
    assert_eq!(raw_fid.metadata.nucleus, "13C");
    assert_relative_eq!(raw_fid.metadata.obs_freq_mhz, 99.54517646288457, epsilon = 1e-6);
    assert_relative_eq!(raw_fid.metadata.spectral_width_hz, 31250.0, epsilon = 1e-6);
    assert_relative_eq!(raw_fid.metadata.center_ppm, 100.0, epsilon = 1e-6);
    assert_eq!(raw_fid.group_delay, Some(19.6875));

    let settings = FtSettings {
        window: WindowFunction::Exponential { lb: 0.12 },
        zf_factor: 4,
        remove_digital_filter: true,
    };

    let p0 = 35.0;
    let p1 = -15.0;
    let processed = process_raw_fid(&raw_fid, &settings, p0, p1).expect("Processing failed");

    let n = processed.ppm.len();
    assert_eq!(n, 131072);
    assert_eq!(processed.spectrum_real.len(), 131072);

    // PPM軸の検証 (Python版: 256.96390880200795 .. -56.96151372283305)
    assert_relative_eq!(processed.ppm[0], 256.96390880200795, epsilon = 1e-8);
    assert_relative_eq!(processed.ppm[n - 1], -56.96151372283305, epsilon = 1e-8);

    // スペクトル先頭の点検証 (Python版: [-7.78427281, -0.33137229, 7.60825539, 2.21387298, -4.71273276])
    assert_relative_eq!(processed.spectrum_real[0], -7.78427281, max_relative = 1e-5);
    assert_relative_eq!(processed.spectrum_real[1], -0.33137229, max_relative = 1e-5);
    assert_relative_eq!(processed.spectrum_real[2], 7.60825539, max_relative = 1e-5);
    assert_relative_eq!(processed.spectrum_real[3], 2.21387298, max_relative = 1e-5);
    assert_relative_eq!(processed.spectrum_real[4], -4.71273276, max_relative = 1e-5);

    // 全体の総和・極値の検証 (Python版: sum = -1881247.498015838, min = -43470.16688342145, max = 14578.941580392298)
    let sum: f64 = processed.spectrum_real.iter().sum();
    let min = processed.spectrum_real.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = processed.spectrum_real.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    assert_relative_eq!(sum, -1881247.498015838, max_relative = 1e-5);
    assert_relative_eq!(min, -43470.16688342145, max_relative = 1e-5);
    assert_relative_eq!(max, 14578.941580392298, max_relative = 1e-5);
}
