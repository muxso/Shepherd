use super::UserError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserTheme {
    pub mode: String,
    pub primary_color: String,
}
impl UserTheme {
    pub fn new(mode: &str, color: &str) -> Result<Self, UserError> {
        if !matches!(mode, "light" | "dark")
            || color.len() != 7
            || !color.starts_with('#')
            || !color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
        {
            return Err(UserError::InvalidTheme);
        }
        Ok(Self { mode: mode.into(), primary_color: color.to_ascii_uppercase() })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn themes_accept_only_supported_modes_and_six_digit_colors() {
        assert_eq!(
            UserTheme::new("light", "#4d9f92").expect("valid fixture").primary_color,
            "#4D9F92"
        );
        for (mode, color) in [
            ("system", "#4d9f92"),
            ("dark", "red"),
            ("dark", "#fff"),
            ("light", "#000000;display:none"),
            ("dark", "#💚12"),
        ] {
            assert!(UserTheme::new(mode, color).is_err());
        }
    }
}
