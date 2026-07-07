use ratatui::style::Color;

pub struct Theme {
    pub background: Color,
    pub surface: Color,
    pub border: Color,
    pub border_focused: Color,
    pub accent: Color,
    pub text: Color,
    pub muted_text: Color,
}
impl Default for Theme {
    fn default() -> Self {
        Self {
            background: Color::Rgb(11, 14, 20),
            surface: Color::Rgb(17, 20, 28),
            border: Color::Rgb(42, 47, 58),
            border_focused: Color::Rgb(79, 163, 209),
            accent: Color::Rgb(79, 163, 209),
            text: Color::Rgb(201, 209, 217),
            muted_text: Color::Rgb(107, 114, 128),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn default_theme_constructs_without_panicking() {
        let theme = Theme::default();
        assert_eq!(theme.background, Color::Rgb(11, 14, 20));
        assert_eq!(theme.surface, Color::Rgb(17, 20, 28));
        assert_eq!(theme.border, Color::Rgb(42, 47, 58));
        assert_eq!(theme.border_focused, Color::Rgb(79, 163, 209));
        assert_eq!(theme.accent, Color::Rgb(79, 163, 209));
        assert_eq!(theme.text, Color::Rgb(201, 209, 217));
        assert_eq!(theme.muted_text, Color::Rgb(107, 114, 128));
    }

    #[test]
    fn default_theme_semantic_colors_are_distinct_except_intentional() {
        let theme = Theme::default();

        assert_ne!(theme.background, theme.surface);
        assert_ne!(theme.background, theme.border);
        assert_ne!(theme.surface, theme.border);
        assert_ne!(theme.border, theme.text);
        assert_ne!(theme.text, theme.muted_text);

        // intentional
        assert_eq!(theme.border_focused, theme.accent);
    }
}
