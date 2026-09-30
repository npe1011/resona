use resona::{
    process_raw_fid, FtSettings, JeolJdfReader, NmrDataSource, WindowFunction,
};

fn count_oscillations_and_max_jump(spectrum: &[f64]) -> (usize, f64, f64) {
    let window = &spectrum[spectrum.len() - 200..spectrum.len() - 100];
    let mut sign_flips = 0;
    let mut max_jump: f64 = 0.0;
    let mut sum_sq_diff: f64 = 0.0;

    for i in 0..window.len() - 2 {
        let d1 = window[i + 1] - window[i];
        let d2 = window[i + 2] - window[i + 1];
        if d1 * d2 < 0.0 {
            sign_flips += 1;
        }
        let jump = d1.abs();
        if jump > max_jump {
            max_jump = jump;
        }
        sum_sq_diff += d1 * d1;
    }
    let rms_diff = (sum_sq_diff / (window.len() - 1) as f64).sqrt();
    (sign_flips, max_jump, rms_diff)
}

#[test]
fn test_proton2_ft_baseline_smoothness() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("test_data").join("Proton2.jdf");
    let raw_fid = JeolJdfReader::read_fid(&path).expect("Failed to read Proton2.jdf");

    let settings = FtSettings {
        window: WindowFunction::Exponential { lb: 0.12 },
        zf_factor: 2,
        remove_digital_filter: true,
    };

    let processed = process_raw_fid(&raw_fid, &settings, 0.0, 0.0)
        .expect("Failed to process Proton2.jdf");
    let spectrum = processed.spectrum_real.as_slice().unwrap();
    let (sign_flips, max_jump, rms_diff) = count_oscillations_and_max_jump(spectrum);
    println!(
        "process_raw_fid Proton2 (ZF x2): flips={}/97, max_jump={:.4}, rms_diff={:.4}",
        sign_flips, max_jump, rms_diff
    );

    // 以前のバグでは max_jump が 7.7 以上、rms_diff が 7.4 以上の激しいギザギザが発生していた
    // 修正後は max_jump < 0.3, rms_diff < 0.2 の滑らかなノイズレベルに収まることをアサート
    assert!(
        max_jump < 0.3,
        "Baseline jump too high ({:.4} >= 0.3), baseline wiggling detected",
        max_jump
    );
    assert!(
        rms_diff < 0.2,
        "Baseline RMS diff too high ({:.4} >= 0.2), baseline wiggling detected",
        rms_diff
    );
}

#[test]
fn test_proton1_ft_runs_successfully() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("test_data").join("Proton1.jdf");
    let raw_fid = JeolJdfReader::read_fid(&path).expect("Failed to read Proton1.jdf");

    let settings = FtSettings {
        window: WindowFunction::Exponential { lb: 0.12 },
        zf_factor: 2,
        remove_digital_filter: true,
    };

    let processed = process_raw_fid(&raw_fid, &settings, 0.0, 0.0)
        .expect("Failed to process Proton1.jdf");

    assert_eq!(processed.spectrum_real.len(), raw_fid.metadata.points * 2);
}



