/// Resona GUI の操作モード定義

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Phase,
    Baseline,
    Reference,
    Peak,
    Integrate,
    Multiview,
    JCoupling,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoomTool {
    Rect,
    X,
    Y,
}

// 互換性のためのエイリアス
pub type ZoomSubMode = ZoomTool;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PeakSubMode {
    #[default]
    None,
    Threshold,
    Add,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IntegrateSubMode {
    #[default]
    None,
    Add,
    Edit,
    Split,
    Delete,
    Reference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MultiviewSubMode {
    #[default]
    None,
    Add,
    Edit,
    Delete,
}
