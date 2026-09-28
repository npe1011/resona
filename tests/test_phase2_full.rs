use resona::{
    analyze_multiplet, auto_detect_integrations, estimate_noise_mad, pick_peaks,
    snap_and_add_peak, JCouplingResultItem, MultiviewItem, Project, RectF,
};

#[test]
#[ignore = "File I/O restricted in environment; run manually by user"]
fn test_phase2_full_pipeline_and_persistence() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("test_data/Proton1.jdf");

    // 1. プロジェクトのロード
    let mut project = Project::new();
    project.load_jdf(&path, None).expect("Load JDF failed");

    assert!(project.ppm.is_some());
    assert!(project.spectrum_real.is_some());
    assert!(project.complex_spectrum_unphased.is_some());
    assert!(project.fid_raw.is_some());

    // 2. ALS ベースライン補正の適用
    project.auto_baseline(1e8, 0.005);
    assert!(project.baseline_array.is_some());

    // 3. ピークピッキング
    let spec = project.spectrum_real.as_ref().unwrap();
    let ppm = project.ppm.as_ref().unwrap();
    let noise = estimate_noise_mad(spec);
    let thresh = noise * 10.0;
    project.state.peak_threshold = Some(thresh);

    let peaks = pick_peaks(spec, ppm, thresh, &project.state.peaks);
    assert!(!peaks.is_empty(), "Peaks should be detected");
    project.state.peaks = peaks;

    // 4. 手動ピークの追加と保護テスト
    let manual_ppm = 7.26;
    let peaks_with_manual = snap_and_add_peak(spec, ppm, manual_ppm, &project.state.peaks);
    assert!(peaks_with_manual.iter().any(|p| !p.is_auto));
    project.state.peaks = peaks_with_manual;

    // 再度自動ピックを実行しても手動ピークが残っていること
    let repicked = pick_peaks(spec, ppm, thresh, &project.state.peaks);
    assert!(repicked.iter().any(|p| !p.is_auto), "Manual peak must be preserved");
    project.state.peaks = repicked;

    // 5. 自動積分
    let intg_items = auto_detect_integrations(spec, ppm);
    assert!(!intg_items.is_empty(), "Integrations should be detected");
    project.state.integrations = intg_items;

    // 6. J-coupling 解析
    let j_candidates = analyze_multiplet(
        vec![2900.0, 2907.5],
        vec![50.0, 50.0],
        "7.26",
        "7.25-7.27",
        "1H",
        1.0,
    );
    assert!(!j_candidates.is_empty());
    project.state.j_couplings.push(JCouplingResultItem {
        text: j_candidates[0].text.clone(),
        ppm: 7.26,
    });

    // 7. Multiview インセット追加
    project.state.multiviews.push(MultiviewItem {
        id: "mv-test".to_string(),
        src_x_min: 7.20,
        src_x_max: 7.35,
        src_y_min: None,
        src_y_max: None,
        ratio: 5.0,
        geometry: RectF { x: 50.0, y: 50.0, w: 200.0, h: 150.0 },
    });

    // 8. リファレンスシフト (0.1 ppm シフト)
    let orig_mv_min = project.state.multiviews[0].src_x_min;
    project.set_shift_reference(0.0, 0.1);
    approx::assert_relative_eq!(project.state.multiviews[0].src_x_min, orig_mv_min + 0.1, epsilon = 1e-10);

    // 9. .rsn (ZIP+JSON+NPY) 保存
    let save_path = manifest_dir.join("target").join("test_save_project.rsn");
    project.save_rsn(&save_path).expect("Save .rsn failed");
    assert!(save_path.exists());

    // 10. 別インスタンスで .rsn ロード
    let mut loaded_project = Project::new();
    loaded_project.load_rsn(&save_path).expect("Load .rsn failed");

    // 検証: 保存前とロード後で全状態が完全一致すること
    assert_eq!(loaded_project.state.peaks.len(), project.state.peaks.len());
    assert_eq!(loaded_project.state.integrations.len(), project.state.integrations.len());
    assert_eq!(loaded_project.state.multiviews.len(), project.state.multiviews.len());
    assert_eq!(loaded_project.state.j_couplings.len(), project.state.j_couplings.len());
    approx::assert_relative_eq!(loaded_project.state.p0, project.state.p0, epsilon = 1e-10);
    approx::assert_relative_eq!(loaded_project.state.p1, project.state.p1, epsilon = 1e-10);

    let orig_spec = project.spectrum_real.as_ref().unwrap();
    let loaded_spec = loaded_project.spectrum_real.as_ref().unwrap();
    assert_eq!(loaded_spec.len(), orig_spec.len());
    for i in 0..100 {
        approx::assert_relative_eq!(loaded_spec[i], orig_spec[i], epsilon = 1e-10);
    }

    // 11. Undo / Redo の動作確認
    let prev_p0 = project.state.p0;
    project.update_phase(prev_p0 + 10.0, project.state.p1);
    project.push_history();
    assert_ne!(project.state.p0, prev_p0);

    let undo_ok = project.undo();
    assert!(undo_ok);
    approx::assert_relative_eq!(project.state.p0, prev_p0, epsilon = 1e-10);

    let redo_ok = project.redo();
    assert!(redo_ok);
    approx::assert_relative_eq!(project.state.p0, prev_p0 + 10.0, epsilon = 1e-10);

    // テスト一時ファイルの片付け
    let _ = std::fs::remove_file(save_path);
}
