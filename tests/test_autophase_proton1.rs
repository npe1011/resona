use resona::{
    autophase_acme, process_raw_fid, FtSettings, JeolJdfReader, NmrDataSource, WindowFunction,
};

#[test]
fn test_acme_autophase_proton1() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("test_data/Proton1.jdf");
    let raw_fid = JeolJdfReader::read_fid(&path).expect("Read failed");

    let settings = FtSettings {
        window: WindowFunction::Exponential { lb: 0.12 },
        zf_factor: 4,
        remove_digital_filter: true,
    };
    let processed = process_raw_fid(&raw_fid, &settings, 0.0, 0.0).expect("Process failed");

    // ACME自動位相補正の実行
    let (p0, p1) = autophase_acme(&processed.complex_spectrum_unphased);

    println!("Proton1 ACME (p0, p1) = ({}, {})", p0, p1);

    // Python/nmrglue版: p0 = -105.30, p1 = 5.35
    assert!((p0 - (-105.3)).abs() < 2.0, "p0 expected ~ -105.3, got {}", p0);
    assert!((p1 - 5.35).abs() < 2.0, "p1 expected ~ 5.35, got {}", p1);
}
