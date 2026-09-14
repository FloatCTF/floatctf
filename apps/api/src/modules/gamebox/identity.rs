//! GameBox 身份纯函数（safe_name 校验 / slug 生成）。

/// safe_name 校验规则：`^[a-z0-9][a-z0-9_-]*$`，与 Challenge safe_name 对齐。
pub fn validate_safe_name(s: &str) -> Result<(), String> {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return Err("safe_name 不能为空".into());
    }
    let first_ok = bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit();
    if !first_ok {
        return Err(format!("safe_name 必须以小写字母或数字开头: {s}"));
    }
    for &b in bytes {
        let ok = b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_';
        if !ok {
            return Err(format!(
                "safe_name 只允许 [a-z0-9_-]: {s}（字符 0x{:02x}）",
                b
            ));
        }
    }
    Ok(())
}

/// 由展示名生成 safe_name 候选（小写 + 非字母数字转 -）。
pub fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_dash = false;
    for ch in name.chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_name_validation_rules() {
        assert!(validate_safe_name("easy-web").is_ok());
        assert!(validate_safe_name("easy_web").is_ok());
        assert!(validate_safe_name("easyweb1").is_ok());
        assert!(validate_safe_name("EasyWeb").is_err(), "大写非法");
        assert!(validate_safe_name("1easy").is_ok());
        assert!(validate_safe_name("-easy").is_err(), "不能以 - 开头");
        assert!(validate_safe_name("easy web").is_err(), "空格非法");
        assert!(validate_safe_name("").is_err());
    }

    #[test]
    fn slugify_basics() {
        assert_eq!(slugify("Easy Web"), "easy-web");
        assert_eq!(slugify("Pwn 01"), "pwn-01");
        assert_eq!(slugify("  Misc "), "misc");
        assert_eq!(slugify("Already_snake"), "already_snake");
        assert_eq!(slugify("a--b"), "a-b");
    }
}
