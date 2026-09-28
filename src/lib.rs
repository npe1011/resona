pub mod core;
pub mod gui;

pub use core::{
    analysis::{
        analyze_multiplet, auto_detect_integrations, compute_integral, estimate_noise_mad,
        pick_peaks, snap_and_add_peak, add_peak_in_range, IntegralResult, IntegrationItem, JCouplingCandidate,
        JCouplingResultItem, PeakItem,
    },
    autophase::{acme_score, autophase_acme, nelder_mead_2d},
    baseline::{apply_baseline_correction, baseline_als},
    error::{ResonaError, Result},
    io::{AcquisitionMetadata, JeolJdfReader, NmrDataSource, RawFid},
    pipeline::{process_raw_fid, FtSettings, ProcessedSpectrum},
    project::{DisplaySettings, HistoryManager, MultiviewItem, Project, ProjectState, RectF},
    signal::{
        apply_phase_and_extract_real, apply_phase_complex, apply_window, apply_zerofill,
        compute_ppm_scale, compute_window_curve, forward_fft, remove_fractional_delay,
        WindowFunction,
    },
};
pub use gui::ResonaApp;
