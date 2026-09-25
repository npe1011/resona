use std::time::Instant;
use resona::{
    apply_baseline_correction, process_raw_fid, FtSettings, JeolJdfReader, NmrDataSource,
    WindowFunction,
};

#[test]
fn test_als_proton1_performance_and_result() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("test_data/Proton1.jdf");
    let raw_fid = JeolJdfReader::read_fid(&path).expect("Read failed");

    let settings = FtSettings {
        window: WindowFunction::Exponential { lb: 0.12 },
        zf_factor: 4,
        remove_digital_filter: true,
        auto_phase: false,
    };
    let processed = process_raw_fid(&raw_fid, &settings, 35.0, -15.0).expect("Process failed");

    assert_eq!(processed.spectrum_real.len(), 65536);

    let start = Instant::now();
    let (corrected, baseline) = apply_baseline_correction(&processed.spectrum_real, 1e8, 0.005);
    let elapsed = start.elapsed();

    println!("ALS 65536 pts elapsed: {:?}", elapsed);

    assert_eq!(corrected.len(), 65536);
    assert_eq!(baseline.len(), 65536);

    // 64k点のALSが 500ms 以内に完了すること
    assert!(elapsed.as_millis() < 500);

    // 補正後スペクトル = 元スペクトル - ベースライン
    for i in 0..10 {
        approx::assert_relative_eq!(
            corrected[i],
            processed.spectrum_real[i] - baseline[i],
            epsilon = 1e-10
        );
    }
}
