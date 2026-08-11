//! 敏感值包装类型：在日志与 `Debug` 中脱敏。

use std::fmt;

/// 不透明密钥字符串——永不在 Debug/Display 中打印。
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Expose the raw secret for crypto use only. Prefer not logging this.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_redacts_debug_and_display() {
        let s = Secret::new("super-secret-value");
        assert!(!format!("{s:?}").contains("super-secret"));
        assert_eq!(format!("{s}"), "***");
        assert_eq!(s.expose(), "super-secret-value");
    }
}
