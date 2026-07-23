use serde_json::Value;

/// Char-safe truncation. JS uses UTF-16 code-unit length; we use Unicode scalar
/// count. The offset differs slightly for astral chars but this is purely
/// decorative truncation — never slicing on a byte boundary avoids CJK panics.
pub fn truncate(text: &str, max_length: usize) -> String {
    if text.chars().count() <= max_length {
        return text.to_string();
    }
    let head: String = text.chars().take(max_length).collect();
    format!("{head}...")
}

/// Mirror of TS `previewValue`: strings are truncated directly, everything else
/// is JSON-stringified then truncated.
pub fn preview_value(value: &Value, max_length: usize) -> String {
    if let Value::String(s) = value {
        return truncate(s, max_length);
    }
    match serde_json::to_string(value) {
        Ok(s) => truncate(&s, max_length),
        Err(_) => truncate(&value.to_string(), max_length),
    }
}

/// `previewValue` over a possibly-absent property. Absent → JS `undefined`,
/// which stringifies to the literal `"undefined"`.
pub fn preview_opt(value: Option<&Value>, max_length: usize) -> String {
    match value {
        Some(v) => preview_value(v, max_length),
        None => truncate("undefined", max_length),
    }
}

/// Format a JSON number the way JS `${n}` does: integer-valued numbers render
/// without a decimal point, fractional ones use the shortest round-trip form.
pub fn fmt_num(n: f64) -> String {
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}
