#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InterpMode {
    #[default]
    Nearest,
    Linear,
    Bicubic,
}

impl InterpMode {
    pub fn label(self) -> &'static str {
        match self {
            InterpMode::Nearest => "nearest",
            InterpMode::Linear  => "linear",
            InterpMode::Bicubic => "bicubic",
        }
    }

    pub fn next(self) -> Self {
        match self {
            InterpMode::Nearest => InterpMode::Linear,
            InterpMode::Linear  => InterpMode::Bicubic,
            InterpMode::Bicubic => InterpMode::Nearest,
        }
    }

    pub fn as_u32(self) -> u32 {
        match self {
            InterpMode::Nearest => 0,
            InterpMode::Linear  => 1,
            InterpMode::Bicubic => 2,
        }
    }
}
