pub mod analysis;
pub mod autophase;
pub mod baseline;
pub mod error;
pub mod io;
pub mod pipeline;
pub mod project;
pub mod signal;

pub use analysis::{
    add_peak_in_range, analyze_multiplet, auto_detect_integrations, auto_detect_reference_peak,
    compute_integral, estimate_noise_mad, parse_jcoupling_sort_ppm, pick_peaks,
    resolve_solvent_target_ppm, snap_and_add_peak, AutoSensitivity, IntegralResult,
    IntegrationItem, JCouplingCandidate, JCouplingResultItem, PeakItem, SolventInfo, KNOWN_SOLVENTS,
};
pub use autophase::{acme_score, autophase_acme, autophase_acme_with_pivot, nelder_mead_2d};
pub use baseline::{
    apply_baseline_correction, apply_baseline_method, baseline_airpls, baseline_als,
    baseline_polynomial, BaselineMethod,
};
pub use error::{ResonaError, Result};
pub use io::{AcquisitionMetadata, BrukerReader, JeolJdfReader, NmrDataSource, RawFid};
pub use pipeline::{process_raw_fid, FtSettings, ProcessedSpectrum};

pub use project::{
    DisplaySettings, FullAutoReport, HistoryManager, MultiviewItem, Project, ProjectState, RectF,
};
pub use signal::{
    apply_phase_and_extract_real, apply_phase_complex, apply_window, apply_zerofill,
    compute_ppm_scale, compute_window_curve, find_highest_peak_in_range, forward_fft,
    remove_fractional_delay, WindowFunction,
};
