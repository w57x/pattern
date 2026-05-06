#[derive(Debug, Clone)]
pub struct BlurConfig {
    pub enabled: bool,
    pub size: u32,
    pub passes: u32,
    pub vibrancy: f32,
}

impl Default for BlurConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            size: 3,
            passes: 1,
            vibrancy: 0.1696,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ShadowConfig {
    pub enabled: bool,
    pub range: u32,
    pub render_power: u32,
    pub color: [f32; 4],
    pub sharp: bool,
    pub offset: (f64, f64),
}

impl Default for ShadowConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            range: 10,
            render_power: 1,
            color: [0.1, 0.1, 0.1, 0.1],
            sharp: false,
            offset: (0.0, 0.0),
        }
    }
}

#[derive(Debug, Clone)]
pub struct StyleConfig {
    pub active_opacity: f64,
    pub inactive_opacity: f64,
    pub fullscreen_opacity: f64,
    pub dim_inactive: bool,
    pub dim_strength: f64,
    pub rounding: f32, // border radius
    pub blur: BlurConfig,
    pub shadow: ShadowConfig,
}

impl Default for StyleConfig {
    fn default() -> Self {
        Self {
            active_opacity: 1.0,
            inactive_opacity: 1.0,
            fullscreen_opacity: 1.0,
            dim_inactive: false,
            dim_strength: 0.5,
            rounding: 14.0,
            blur: BlurConfig::default(),
            shadow: ShadowConfig::default(),
        }
    }
}
