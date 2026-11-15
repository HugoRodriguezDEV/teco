//! Formateo pequeño sin dependencias: bytes, duraciones, JSON y fechas.

use std::fmt::Write;
use std::time::{SystemTime, UNIX_EPOCH};

pub const GB: f64 = 1024.0 * 1024.0 * 1024.0;

pub fn fmt_bytes(b: u64) -> String {
    let b = b as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else {
        format!("{:.0} MB", b / (1024.0 * 1024.0))
    }
}

pub fn fmt_dur(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs} s"),
        60..=3599 => format!("{} min", secs / 60),
        3600..=86_399 => format!("{} h {} min", secs / 3600, (secs % 3600) / 60),
        _ => format!("{} d {} h", secs / 86_400, (secs % 86_400) / 3600),
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// `2026-11-15T20:56:01Z` a partir de milisegundos Unix (UTC).
pub fn iso_utc(ms: u64) -> String {
    let secs = ms / 1000;
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Algoritmo civil_from_days de Howard Hinnant.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Escribe `s` como string JSON (con comillas).
pub fn json_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c == '\u{2028}' || c == '\u{2029}' => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            // Evita que "</script>" o similares se interpreten si alguien incrusta el JSON.
            '<' => out.push_str("\\u003c"),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Número JSON (NaN/inf no existen en JSON → 0).
pub fn json_num(out: &mut String, v: f64) {
    if v.is_finite() {
        if v.fract() == 0.0 && v.abs() < 1e15 {
            let _ = write!(out, "{}", v as i64);
        } else {
            let _ = write!(out, "{v:.2}");
        }
    } else {
        out.push('0');
    }
}

/// Recorta a `max` caracteres respetando UTF-8.
pub fn truncate(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso() {
        assert_eq!(iso_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_utc(1_763_240_161_000), "2025-11-15T20:56:01Z");
    }

    #[test]
    fn json() {
        let mut s = String::new();
        json_str(&mut s, "a\"b\\c\n</x>");
        assert_eq!(s, r#""a\"b\\c\n\u003c/x>""#);
        let mut n = String::new();
        json_num(&mut n, 3.0);
        n.push(',');
        json_num(&mut n, 1.234);
        n.push(',');
        json_num(&mut n, f64::NAN);
        assert_eq!(n, "3,1.23,0");
    }

    #[test]
    fn fmt() {
        assert_eq!(fmt_dur(43 * 60 + 5), "43 min");
        assert_eq!(fmt_bytes((3.1 * GB) as u64), "3.1 GB");
        assert_eq!(truncate("ñandú", 3), "ñan");
    }
}
