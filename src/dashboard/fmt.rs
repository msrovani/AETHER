//! Formatador compacto de números para o hub (4 dígitos + k/kk/kkk).

/// Formata com **4 dígitos** + sufixo `k`/`kk`/`kkk` (sem `eN`).
/// Exemplos: `1,586` · `15,86` · `158,6` · `87k` · `235kk` · `3,180kkk` · `0,003182`.
pub fn fmt_compact(n: f64) -> String {
    if !n.is_finite() {
        return "—".into();
    }
    let sign = if n.is_sign_negative() && n != 0.0 {
        "-"
    } else {
        ""
    };
    let mut x = n.abs();
    if x == 0.0 {
        return format!("{sign}0");
    }

    const SUFFIXES: [&str; 5] = ["", "k", "kk", "kkk", "kkkk"];
    let mut scale = 0usize;
    while x >= 1000.0 && scale + 1 < SUFFIXES.len() {
        x /= 1000.0;
        scale += 1;
    }
    let suffix = SUFFIXES[scale];

    if x < 1.0 {
        return format!("{sign}{}", four_digits_lt1(x));
    }

    format!("{sign}{}{suffix}", four_digits_ge1(x))
}

fn four_digits_ge1(x: f64) -> String {
    let raw = if x < 10.0 {
        format!("{x:.3}")
    } else if x < 100.0 {
        format!("{x:.2}")
    } else {
        format!("{x:.1}")
    };
    trim_comma_zeros(&raw.replace('.', ","))
}

fn four_digits_lt1(x: f64) -> String {
    let exp = x.log10().floor() as i32;
    let frac_digits = ((-exp) + 3).clamp(3, 9) as usize;
    let raw = format!("{x:.frac_digits$}");
    trim_comma_zeros(&raw.replace('.', ","))
}

fn trim_comma_zeros(s: &str) -> String {
    if !s.contains(',') {
        return s.to_string();
    }
    let t = s.trim_end_matches('0').trim_end_matches(',');
    if t.is_empty() {
        "0".into()
    } else {
        t.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_compact_milhares_e_fracoes() {
        assert_eq!(fmt_compact(1.586), "1,586");
        assert_eq!(fmt_compact(15.86), "15,86");
        assert_eq!(fmt_compact(158.6), "158,6");
        assert_eq!(fmt_compact(87_000.0), "87k");
        assert_eq!(fmt_compact(235_000_000.0), "235kk");
        assert_eq!(fmt_compact(0.0), "0");
    }
}
