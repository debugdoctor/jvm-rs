use crate::vm::{Value, VmError};

pub(super) fn format_unsigned_radix(mut value: u64, radix: u32) -> String {
    if value == 0 {
        return "0".to_string();
    }
    let digits = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut buf = Vec::new();
    while value > 0 {
        buf.push(digits[(value % radix as u64) as usize]);
        value /= radix as u64;
    }
    buf.reverse();
    String::from_utf8(buf).unwrap()
}

pub(super) fn format_float(v: f64) -> String {
    if v.is_nan() {
        "NaN".to_string()
    } else if v.is_infinite() {
        if v > 0.0 {
            "Infinity".to_string()
        } else {
            "-Infinity".to_string()
        }
    } else if v == 0.0 && v.is_sign_negative() {
        "-0.0".to_string()
    } else {
        let s = format!("{v}");
        if s.contains('.') { s } else { format!("{v}.0") }
    }
}
