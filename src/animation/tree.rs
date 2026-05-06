use super::Curve;

#[derive(Debug, Clone)]
pub struct AnimationStyle {
    pub duration: f64, // ms
    pub curve: Curve,
    pub style_name: String,
    pub enabled: bool,
}

impl Default for AnimationStyle {
    fn default() -> Self {
        Self {
            duration: 300.0,
            curve: Curve::ease_out(),
            style_name: "popin".to_string(),
            enabled: true,
        }
    }
}

impl AnimationStyle {
    pub fn new(duration: f64, curve: Curve, style_name: &str) -> Self {
        Self {
            duration,
            curve,
            style_name: style_name.to_string(),
            enabled: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnimationTree {
    pub windows_in: AnimationStyle,
    pub windows_out: AnimationStyle,
    pub windows_move: AnimationStyle,

    pub layers_in: AnimationStyle,
    pub layers_out: AnimationStyle,

    pub fade_in: AnimationStyle,
    pub fade_out: AnimationStyle,
    pub fade_layers_in: AnimationStyle,
    pub fade_layers_out: AnimationStyle,
    pub fade_popups_in: AnimationStyle,
    pub fade_popups_out: AnimationStyle,

    pub workspaces_in: AnimationStyle,
    pub workspaces_out: AnimationStyle,
}

impl Default for AnimationTree {
    fn default() -> Self {
        Self {
            windows_in: AnimationStyle::new(300.0, Curve::bouncy(), "popin"),
            windows_out: AnimationStyle::new(300.0, Curve::ease_out(), "popin"), // usually fade out is smooth
            windows_move: AnimationStyle::new(300.0, Curve::bouncy(), "slide"),

            layers_in: AnimationStyle::new(200.0, Curve::ease_out(), "fade"),
            layers_out: AnimationStyle::new(200.0, Curve::ease_out(), "fade"),

            fade_in: AnimationStyle::new(200.0, Curve::ease_out(), "fade"),
            fade_out: AnimationStyle::new(200.0, Curve::ease_out(), "fade"),
            fade_layers_in: AnimationStyle::new(200.0, Curve::ease_out(), "fade"),
            fade_layers_out: AnimationStyle::new(200.0, Curve::ease_out(), "fade"),
            fade_popups_in: AnimationStyle::new(150.0, Curve::ease_out(), "fade"),
            fade_popups_out: AnimationStyle::new(150.0, Curve::ease_out(), "fade"),

            workspaces_in: AnimationStyle::new(400.0, Curve::ease_out(), "slide"),
            workspaces_out: AnimationStyle::new(400.0, Curve::ease_out(), "slide"),
        }
    }
}
