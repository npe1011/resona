pub mod about_dialog;
pub mod display_dialog;
pub mod ft_dialog;
pub mod full_auto_dialog;
pub mod jcoupling_dialog;
pub mod multiview_dialog;
pub mod peak_list_dialog;
pub mod print_dialog;
pub mod print_style_dialog;

pub use about_dialog::{show_about_dialog, AboutDialogState};
pub use display_dialog::{show_display_dialog, DisplayDialogState, DisplaySettingsResult};
pub use ft_dialog::{show_ft_dialog, FtDialogState};
pub use full_auto_dialog::{
    show_full_auto_dialog, FullAutoBaselineChoice, FullAutoDialogState, FullAutoResult,
};
pub use jcoupling_dialog::{show_jcoupling_dialog, JCouplingDialogState};
pub use multiview_dialog::{
    show_multiview_yscale_dialog, MultiviewYScaleDialogState, MultiviewYScaleResult,
};
pub use peak_list_dialog::{show_peak_list_dialog, PeakListDialogState};
pub use print_dialog::{show_print_dialog, PrintDialogState};
pub use print_style_dialog::{
    show_print_style_dialog, PrintMultiviewStyle, PrintSpectrumStyle, PrintStyleDialogState,
    PrintStyleSettings, RgbColor,
};
