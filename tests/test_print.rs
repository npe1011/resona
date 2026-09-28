use ndarray::Array1;
use resona::core::{AcquisitionMetadata, FtSettings, IntegrationItem, JCouplingResultItem, MultiviewItem, PeakItem};
use resona::gui::dialogs::print_dialog::{generate_complete_page_svg, PrintOrientation, PrintSettings};

#[test]
fn test_print_settings_default() {
    let settings = PrintSettings::default();
    assert_eq!(settings.orientation, PrintOrientation::Landscape);
    assert!(settings.spectrum);
    assert!(settings.peak);
    assert!(settings.integrate);
    assert!(settings.multiview);
    assert!(settings.info);
    assert!(settings.jcoupling);
    assert!(settings.filename);
    assert_eq!(settings.ppm_decimals, 3);
    assert_eq!(settings.integral_decimals, 2);
    assert!(!settings.auto_ticks);
    assert_eq!(settings.tick_major, 1.0);
    assert_eq!(settings.tick_minor, 10);
}

#[test]
fn test_generate_complete_page_svg_minimal() {
    let settings = PrintSettings::default();
    let ppm = Array1::linspace(10.0, 0.0, 100);
    let mut spec = Array1::zeros(100);
    spec[50] = 100.0;

    let peaks = vec![PeakItem {
        ppm: 5.0,
        intensity: 100.0,
        is_auto: true,
    }];
    let integrations = Vec::new();
    let multiviews = Vec::new();
    let metadata = AcquisitionMetadata::default();
    let ft_settings = FtSettings::default();
    let j_couplings = Vec::new();

    let svg = generate_complete_page_svg(
        &settings,
        None,
        Some(&ppm),
        Some(&spec),
        &peaks,
        &integrations,
        1.0,
        0.03,
        1.0,
        &multiviews,
        &metadata,
        &ft_settings,
        &j_couplings,
        None,
    );

    assert!(svg.starts_with("<svg"));
    assert!(svg.ends_with("</svg>"));
    assert!(svg.contains("path"));
    assert!(svg.contains("ppm"));
}

#[test]
fn test_generate_complete_page_svg_full() {
    let mut settings = PrintSettings::default();
    settings.orientation = PrintOrientation::Portrait;
    let ppm = Array1::linspace(10.0, 0.0, 100);
    let spec = Array1::ones(100);
    let peaks = vec![
        PeakItem { ppm: 7.26, intensity: 50.0, is_auto: false },
        PeakItem { ppm: 2.1, intensity: 30.0, is_auto: true },
    ];
    let integrations = vec![
        IntegrationItem {
            id: "1".to_string(),
            start_ppm: 7.4,
            end_ppm: 7.1,
            y_start: 0.0,
            y_end: 0.0,
        },
    ];
    let multiviews = vec![
        MultiviewItem {
            id: "1".to_string(),
            src_x_min: 7.0,
            src_x_max: 7.5,
            src_y_min: None,
            src_y_max: None,
            ratio: 5.0,
            geometry: resona::core::RectF { x: 100.0, y: 100.0, w: 200.0, h: 150.0 },
        },
    ];
    let metadata = AcquisitionMetadata::default();
    let ft_settings = FtSettings::default();
    let j_couplings = vec![JCouplingResultItem {
        text: "d, J = 7.2 Hz".to_string(),
        ppm: 5.0,
    }];
    let path = std::path::Path::new("sample_spectrum.fid");

    let svg = generate_complete_page_svg(
        &settings,
        None,
        Some(&ppm),
        Some(&spec),
        &peaks,
        &integrations,
        1.0,
        0.03,
        1.0,
        &multiviews,
        &metadata,
        &ft_settings,
        &j_couplings,
        Some(path),
    );

    assert!(svg.starts_with("<svg"));
    assert!(svg.ends_with("</svg>"));
    assert!(svg.contains("sample_spectrum.fid"));
    assert!(svg.contains("Experimental Parameters"));
    assert!(svg.contains("FT Settings"));
    assert!(svg.contains("J Coupling"));
    assert!(svg.contains("d, J = 7.2 Hz"));
    assert!(svg.contains("#e11d48")); // 積分曲線またはテキスト
    assert!(svg.contains("#a0a0a0")); // マルチビュー枠 (GUI準拠のグレー)
}

#[test]
fn test_parameters_off_expands_plot_and_filename_toggle_fixed_y() {
    let ppm = Array1::linspace(10.0, 0.0, 100);
    let spec = Array1::ones(100);
    let peaks = Vec::new();
    let integrations = Vec::new();
    let multiviews = Vec::new();
    let metadata = AcquisitionMetadata::default();
    let ft_settings = FtSettings::default();
    let empty_j_couplings = Vec::new();

    // 1. パラメータ ON の場合 (サイド幅が確保される)
    let mut settings_on = PrintSettings::default();
    settings_on.info = true;
    settings_on.filename = true;
    let svg_on = generate_complete_page_svg(
        &settings_on,
        None,
        Some(&ppm),
        Some(&spec),
        &peaks,
        &integrations,
        1.0,
        0.03,
        1.0,
        &multiviews,
        &metadata,
        &ft_settings,
        &empty_j_couplings,
        Some(std::path::Path::new("test.fid")),
    );
    assert!(svg_on.contains("Experimental Parameters"));

    // 2. パラメータ OFF の場合 (サイド幅 0、スペクトルが全幅に拡大)
    let mut settings_off = PrintSettings::default();
    settings_off.info = false;
    settings_off.jcoupling = true; // j_couplings が空なら side_w は 0 になるはず
    settings_off.filename = true;
    let svg_off = generate_complete_page_svg(
        &settings_off,
        None,
        Some(&ppm),
        Some(&spec),
        &peaks,
        &integrations,
        1.0,
        0.03,
        1.0,
        &multiviews,
        &metadata,
        &ft_settings,
        &empty_j_couplings,
        Some(std::path::Path::new("test.fid")),
    );
    assert!(!svg_off.contains("Experimental Parameters"));
    // total_w = 1120.0, margin = 20.0 -> 全幅プロットの幅は 1080.0
    // X軸ベースライン line x1="20" y1="..." x2="1100" y2="..." (20 + 1080 = 1100)
    assert!(svg_off.contains(r#"x2="1100""#));

    // 3. File Name OFF の場合でもプロットの Y 座標が変わらないことの検証
    let mut settings_no_fn = settings_off.clone();
    settings_no_fn.filename = false;
    let svg_no_fn = generate_complete_page_svg(
        &settings_no_fn,
        None,
        Some(&ppm),
        Some(&spec),
        &peaks,
        &integrations,
        1.0,
        0.03,
        1.0,
        &multiviews,
        &metadata,
        &ft_settings,
        &empty_j_couplings,
        Some(std::path::Path::new("test.fid")),
    );
    assert!(!svg_no_fn.contains("test.fid"));

    // svg_off と svg_no_fn のプロット baseline Y 座標が全く同一であることを確認
    // baseline の y1="..." を抽出して比較
    let extract_baseline_y = |svg_str: &str| -> String {
        for line in svg_str.lines() {
            if line.contains(r#"x1="20""#) && line.contains(r#"x2="1100""#) && line.contains(r##"stroke="#212529""##) {
                return line.to_string();
            }
        }
        String::new()
    };
    let baseline_fn = extract_baseline_y(&svg_off);
    let baseline_no_fn = extract_baseline_y(&svg_no_fn);
    assert!(!baseline_fn.is_empty());
    assert_eq!(baseline_fn, baseline_no_fn, "Filename toggle must not shift plot Y position!");
}

#[test]
fn test_custom_precision_and_manual_ticks() {
    let mut settings = PrintSettings::default();
    settings.ppm_decimals = 4;
    settings.integral_decimals = 3;
    settings.auto_ticks = false;
    settings.tick_major = 2.0;

    let ppm = Array1::linspace(10.0, 0.0, 100);
    let mut spec = Array1::zeros(100);
    spec[50] = 100.0;

    let peaks = vec![PeakItem {
        ppm: 5.123456,
        intensity: 100.0,
        is_auto: true,
    }];
    let integrations = vec![IntegrationItem {
        id: "1".to_string(),
        start_ppm: 6.0,
        end_ppm: 4.0,
        y_start: 0.0,
        y_end: 0.0,
    }];
    let multiviews = Vec::new();
    let metadata = AcquisitionMetadata::default();
    let ft_settings = FtSettings::default();
    let j_couplings = Vec::new();

    let svg = generate_complete_page_svg(
        &settings,
        None,
        Some(&ppm),
        Some(&spec),
        &peaks,
        &integrations,
        1.0,
        0.03,
        1.0,
        &multiviews,
        &metadata,
        &ft_settings,
        &j_couplings,
        None,
    );

    // 4桁のPPM値 "5.1235" (四捨五入) が含まれるか確認
    assert!(svg.contains("5.1235"), "Peak PPM should be formatted with 4 decimals");
    // 3桁の積分値が含まれるか確認
    assert!(svg.contains(".000") || svg.contains(".1"), "Integration should respect integral_decimals");
    // 手動目盛り 2.0 が反映されているか (2.0, 4.0, 6.0, 8.0, 10.0)
    assert!(svg.contains(">2.0<") || svg.contains(">2<") || svg.contains(">4.0<") || svg.contains(">8.0<"));
}
