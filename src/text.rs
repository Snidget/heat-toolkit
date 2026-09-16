//! Утилиты разбиения текста на строки с сохранением символов конца строки.

/// Разбить текст на (содержимое, конец_строки) — аналог Python splitlines(keepends=True)
/// с последующим отделением терминатора. Поддерживаются те же границы строк, что в Python.
pub fn split_lines(text: &str) -> Vec<(String, String)> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<(String, String)> = Vec::new();
    let mut start = 0usize;
    let mut chars = text.char_indices().peekable();
    while let Some((idx, ch)) = chars.next() {
        let brk_len: usize = match ch {
            '\r' => {
                if let Some(&(_, nxt)) = chars.peek() {
                    if nxt == '\n' {
                        chars.next();
                        2
                    } else {
                        1
                    }
                } else {
                    1
                }
            }
            '\n' | '\u{000B}' | '\u{000C}' | '\u{001C}' | '\u{001D}' | '\u{001E}' | '\u{0085}'
            | '\u{2028}' | '\u{2029}' => ch.len_utf8(),
            _ => 0,
        };
        if brk_len > 0 {
            let content = text[start..idx].to_string();
            let brk = text[idx..idx + brk_len].to_string();
            out.push((content, brk));
            start = idx + brk_len;
        }
    }
    if start < text.len() {
        out.push((text[start..].to_string(), String::new()));
    }
    out
}

/// Отделить текст от символа(ов) конца строки.
pub fn split_line_ending(text: &str) -> (String, String) {
    if let Some(stripped) = text.strip_suffix("\r\n") {
        (stripped.to_string(), "\r\n".to_string())
    } else if let Some(stripped) = text.strip_suffix('\n') {
        (stripped.to_string(), "\n".to_string())
    } else if let Some(stripped) = text.strip_suffix('\r') {
        (stripped.to_string(), "\r".to_string())
    } else {
        (text.to_string(), String::new())
    }
}

/// Форматирование f64 в стиле C `%g` с заданным количеством значащих цифр.
/// Убирает лишние нули после точки, всегда ставит `.0` у целых чисел,
/// использует `E` для научной записи.
pub fn format_g(value: f64, precision: usize) -> String {
    if value == 0.0 {
        return "0.0".to_string();
    }
    let precision = precision.max(1);
    let p = precision as i32;
    let sci = format!("{:.prec$e}", value, prec = precision - 1);
    let parts: Vec<&str> = sci.splitn(2, 'e').collect();
    let mant = parts[0];
    let exp_str = parts.get(1).copied().unwrap_or("0");
    let exp: i32 = exp_str.parse().unwrap_or(0);

    if exp >= -4 && exp < p {
        let decimals = (p - 1 - exp).max(0) as usize;
        let mut s = format!("{:.*}", decimals, value);
        if s.contains('.') {
            while s.ends_with('0') {
                s.pop();
            }
            if s.ends_with('.') {
                s.pop();
            }
        }
        if !s.contains('.') {
            s.push_str(".0");
        }
        s
    } else {
        let mut mant_s = mant.trim_end_matches('0').to_string();
        if mant_s.ends_with('.') {
            mant_s.pop();
        }
        if !mant_s.contains('.') {
            mant_s += ".0";
        }
        // Python %g добавляет '+' для положительной экспоненты (1e+07)
        let exp_sign = if exp_str.starts_with('-') { "-" } else { "+" };
        let exp_digits = exp_str.trim_start_matches('-').trim_start_matches('+');
        format!("{}E{}{}", mant_s, exp_sign, exp_digits)
    }
}

/// Компактное представление f64 в стиле `%.6g` для STEP-файлов и др.
pub fn format_real(value: f64) -> String {
    format_g(value, 6)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_g_clamps_zero_precision_to_a_safe_value() {
        assert_eq!(format_g(1.25, 0), format_g(1.25, 1));
    }

    #[test]
    fn split_lines_handles_all_multibyte_python_separators() {
        for separator in ['\u{0085}', '\u{2028}', '\u{2029}'] {
            let text = format!("before{separator}after");
            assert_eq!(
                split_lines(&text),
                vec![
                    ("before".to_string(), separator.to_string()),
                    ("after".to_string(), String::new()),
                ]
            );
        }
    }

    #[test]
    fn split_lines_preserves_crlf_as_one_ending() {
        assert_eq!(
            split_lines("first\r\nsecond"),
            vec![
                ("first".to_string(), "\r\n".to_string()),
                ("second".to_string(), String::new()),
            ]
        );
    }
}
