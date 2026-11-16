//! JSON mínimo, sem dependências, só para a borda externa.
//!
//! O núcleo não conhece este módulo: ele existe porque app de IA fala JSON, e
//! o motor fala fase. Converter é trabalho de borda (T2), como o `TransducerIo`.
//!
//! Suporta o subconjunto que uma API precisa: `null`, `bool`, número (`f64`),
//! string com escapes (incl. `\uXXXX` e pares substitutos), array e objeto.
//! Rejeita lixo no fim e profundidade excessiva (sem recursão sem limite).

use std::fmt;

/// Valor JSON.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    /// `null`.
    Null,
    /// `true` / `false`.
    Bool(bool),
    /// Número (sempre `f64`; a borda não precisa de inteiro de 128 bits).
    Num(f64),
    /// String.
    Str(String),
    /// Array.
    Arr(Vec<Json>),
    /// Objeto (ordem preservada).
    Obj(Vec<(String, Json)>),
}

/// Tipo do erro de parse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonErrorKind {
    /// Terminou no meio de um valor.
    UnexpectedEnd,
    /// Byte inesperado na posição.
    UnexpectedByte,
    /// Número malformado.
    InvalidNumber,
    /// Escape inválido em string.
    InvalidEscape,
    /// Sobrou conteúdo depois do valor.
    TrailingData,
    /// Aninhamento acima de [`MAX_DEPTH`].
    TooDeep,
}

/// Profundidade máxima de aninhamento aceita.
pub const MAX_DEPTH: usize = 64;

/// Erro de parse com posição (byte) e tipo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JsonError {
    /// Posição em bytes onde o parse falhou.
    pub at: usize,
    /// Tipo do erro.
    pub kind: JsonErrorKind,
}

impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "json: {:?} em byte {}", self.kind, self.at)
    }
}

impl std::error::Error for JsonError {}

impl Json {
    /// Constrói um objeto a partir de pares.
    pub fn obj<const N: usize>(pairs: [(&str, Json); N]) -> Self {
        Self::Obj(
            pairs
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        )
    }

    /// `Json::Str` a partir de qualquer coisa stringificável.
    pub fn str<S: Into<String>>(s: S) -> Self {
        Self::Str(s.into())
    }

    /// `Json::Num` a partir de `f64` (não finito vira `null`).
    pub fn num(n: f64) -> Self {
        if n.is_finite() {
            Self::Num(n)
        } else {
            Self::Null
        }
    }

    /// Interpreta `text` como um único valor JSON.
    pub fn parse(text: &str) -> Result<Self, JsonError> {
        let mut p = Parser {
            b: text.as_bytes(),
            i: 0,
        };
        p.ws();
        let v = p.value(0)?;
        p.ws();
        if p.i != p.b.len() {
            return Err(p.err(JsonErrorKind::TrailingData));
        }
        Ok(v)
    }

    /// Campo de objeto (ou `None`).
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Self::Obj(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// Elemento de array.
    pub fn idx(&self, i: usize) -> Option<&Json> {
        match self {
            Self::Arr(v) => v.get(i),
            _ => None,
        }
    }

    /// `Some(f64)` se for número.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Num(n) => Some(*n),
            _ => None,
        }
    }

    /// `Some(i64)` se for número inteiro representável.
    pub fn as_i64(&self) -> Option<i64> {
        self.as_f64().and_then(|n| {
            if n.fract() == 0.0 && n.abs() <= 9.22e18 {
                Some(n as i64)
            } else {
                None
            }
        })
    }

    /// `Some(u64)` se for número inteiro não negativo representável.
    pub fn as_u64(&self) -> Option<u64> {
        self.as_i64().and_then(|n| u64::try_from(n).ok())
    }

    /// `Some(&str)` se for string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s),
            _ => None,
        }
    }

    /// `Some(bool)` se for booleano.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// `Some(&[Json])` se for array.
    pub fn as_arr(&self) -> Option<&[Json]> {
        match self {
            Self::Arr(v) => Some(v),
            _ => None,
        }
    }

    /// `Some(&[(String, Json)])` se for objeto.
    pub fn as_obj(&self) -> Option<&[(String, Json)]> {
        match self {
            Self::Obj(v) => Some(v),
            _ => None,
        }
    }

    /// Serializa em texto JSON (compacto).
    pub fn to_text(&self) -> String {
        let mut out = String::with_capacity(64);
        self.write(&mut out);
        out
    }

    fn write(&self, out: &mut String) {
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(true) => out.push_str("true"),
            Self::Bool(false) => out.push_str("false"),
            Self::Num(n) => {
                if !n.is_finite() {
                    out.push_str("null");
                } else if n.fract() == 0.0 && n.abs() < 1e15 {
                    // inteiro: evita "4.0" no JSON
                    out.push_str(&format!("{}", *n as i64));
                } else {
                    out.push_str(&format!("{n}"));
                }
            }
            Self::Str(s) => write_str(out, s),
            Self::Arr(v) => {
                out.push('[');
                for (i, item) in v.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.write(out);
                }
                out.push(']');
            }
            Self::Obj(pairs) => {
                out.push('{');
                for (i, (k, v)) in pairs.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_str(out, k);
                    out.push(':');
                    v.write(out);
                }
                out.push('}');
            }
        }
    }
}

impl fmt::Display for Json {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_text())
    }
}

fn write_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn err(&self, kind: JsonErrorKind) -> JsonError {
        JsonError { at: self.i, kind }
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json, JsonError> {
        if depth > MAX_DEPTH {
            return Err(self.err(JsonErrorKind::TooDeep));
        }
        match self.peek() {
            None => Err(self.err(JsonErrorKind::UnexpectedEnd)),
            Some(b'n') => {
                self.literal(b"null")?;
                Ok(Json::Null)
            }
            Some(b't') => {
                self.literal(b"true")?;
                Ok(Json::Bool(true))
            }
            Some(b'f') => {
                self.literal(b"false")?;
                Ok(Json::Bool(false))
            }
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b'[') => {
                self.i += 1;
                let mut v = Vec::new();
                self.ws();
                if self.peek() == Some(b']') {
                    self.i += 1;
                    return Ok(Json::Arr(v));
                }
                loop {
                    self.ws();
                    v.push(self.value(depth + 1)?);
                    self.ws();
                    match self.peek() {
                        Some(b',') => self.i += 1,
                        Some(b']') => {
                            self.i += 1;
                            return Ok(Json::Arr(v));
                        }
                        Some(_) => return Err(self.err(JsonErrorKind::UnexpectedByte)),
                        None => return Err(self.err(JsonErrorKind::UnexpectedEnd)),
                    }
                }
            }
            Some(b'{') => {
                self.i += 1;
                let mut v: Vec<(String, Json)> = Vec::new();
                self.ws();
                if self.peek() == Some(b'}') {
                    self.i += 1;
                    return Ok(Json::Obj(v));
                }
                loop {
                    self.ws();
                    if self.peek() != Some(b'"') {
                        return Err(self.err(JsonErrorKind::UnexpectedByte));
                    }
                    let k = self.string()?;
                    self.ws();
                    if self.peek() != Some(b':') {
                        return Err(self.err(JsonErrorKind::UnexpectedByte));
                    }
                    self.i += 1;
                    self.ws();
                    let val = self.value(depth + 1)?;
                    v.push((k, val));
                    self.ws();
                    match self.peek() {
                        Some(b',') => self.i += 1,
                        Some(b'}') => {
                            self.i += 1;
                            return Ok(Json::Obj(v));
                        }
                        Some(_) => return Err(self.err(JsonErrorKind::UnexpectedByte)),
                        None => return Err(self.err(JsonErrorKind::UnexpectedEnd)),
                    }
                }
            }
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.err(JsonErrorKind::UnexpectedByte)),
        }
    }

    fn literal(&mut self, want: &[u8]) -> Result<(), JsonError> {
        if self.b.len() >= self.i + want.len() && &self.b[self.i..self.i + want.len()] == want {
            self.i += want.len();
            Ok(())
        } else {
            Err(self.err(JsonErrorKind::UnexpectedByte))
        }
    }

    fn number(&mut self) -> Result<Json, JsonError> {
        let start = self.i;
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        let mut digits = 0usize;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.i += 1;
            digits += 1;
        }
        if digits == 0 {
            return Err(self.err(JsonErrorKind::InvalidNumber));
        }
        if self.peek() == Some(b'.') {
            self.i += 1;
            let mut frac = 0usize;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.i += 1;
                frac += 1;
            }
            if frac == 0 {
                return Err(self.err(JsonErrorKind::InvalidNumber));
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.i += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.i += 1;
            }
            let mut exp = 0usize;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.i += 1;
                exp += 1;
            }
            if exp == 0 {
                return Err(self.err(JsonErrorKind::InvalidNumber));
            }
        }
        let text = std::str::from_utf8(&self.b[start..self.i])
            .map_err(|_| self.err(JsonErrorKind::InvalidNumber))?;
        let n: f64 = text
            .parse()
            .map_err(|_| self.err(JsonErrorKind::InvalidNumber))?;
        Ok(Json::Num(n))
    }

    fn string(&mut self) -> Result<String, JsonError> {
        if self.peek() != Some(b'"') {
            return Err(self.err(JsonErrorKind::UnexpectedByte));
        }
        self.i += 1;
        let mut out = String::new();
        loop {
            let c = match self.peek() {
                Some(c) => c,
                None => return Err(self.err(JsonErrorKind::UnexpectedEnd)),
            };
            match c {
                b'"' => {
                    self.i += 1;
                    return Ok(out);
                }
                b'\\' => {
                    self.i += 1;
                    let e = self.peek().ok_or(self.err(JsonErrorKind::UnexpectedEnd))?;
                    self.i += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{0008}'),
                        b'f' => out.push('\u{000c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hi = self.hex4()?;
                            if (0xD800..0xDC00).contains(&hi) {
                                // par substituto: precisa de \uDC00..\uDFFF
                                if self.peek() == Some(b'\\')
                                    && self.b.get(self.i + 1) == Some(&b'u')
                                {
                                    self.i += 2;
                                    let lo = self.hex4()?;
                                    if (0xDC00..0xE000).contains(&lo) {
                                        let cp = 0x1_0000
                                            + ((hi - 0xD800) << 10)
                                            + (lo - 0xDC00);
                                        out.push(char::from_u32(cp).unwrap_or('\u{fffd}'));
                                    } else {
                                        out.push('\u{fffd}');
                                    }
                                } else {
                                    out.push('\u{fffd}');
                                }
                            } else if (0xDC00..0xE000).contains(&hi) {
                                out.push('\u{fffd}');
                            } else {
                                out.push(char::from_u32(hi).unwrap_or('\u{fffd}'));
                            }
                        }
                        _ => return Err(self.err(JsonErrorKind::InvalidEscape)),
                    }
                }
                c if c < 0x20 => return Err(self.err(JsonErrorKind::UnexpectedByte)),
                _ => {
                    // copia a sequência UTF-8 inteira
                    let rest = &self.b[self.i..];
                    let len = utf8_len(rest[0]);
                    if len == 0 || rest.len() < len {
                        return Err(self.err(JsonErrorKind::UnexpectedByte));
                    }
                    let s = std::str::from_utf8(&rest[..len])
                        .map_err(|_| self.err(JsonErrorKind::UnexpectedByte))?;
                    out.push_str(s);
                    self.i += len;
                }
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, JsonError> {
        if self.b.len() < self.i + 4 {
            return Err(self.err(JsonErrorKind::UnexpectedEnd));
        }
        let mut v = 0u32;
        for _ in 0..4 {
            let c = self.b[self.i];
            let d = match c {
                b'0'..=b'9' => u32::from(c - b'0'),
                b'a'..=b'f' => u32::from(c - b'a') + 10,
                b'A'..=b'F' => u32::from(c - b'A') + 10,
                _ => return Err(self.err(JsonErrorKind::InvalidEscape)),
            };
            v = (v << 4) | d;
            self.i += 1;
        }
        Ok(v)
    }
}

fn utf8_len(first: u8) -> usize {
    match first {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_escalares_e_aninhados() {
        assert_eq!(Json::parse("null").unwrap(), Json::Null);
        assert_eq!(Json::parse("true").unwrap(), Json::Bool(true));
        assert_eq!(Json::parse("-12.5e2").unwrap(), Json::Num(-1250.0));
        assert_eq!(Json::parse(r#""a\nb""#).unwrap(), Json::str("a\nb"));
        let v = Json::parse(r#"{"a":[1,2,{"b":null}],"c":true}"#).unwrap();
        assert_eq!(v.get("a").unwrap().idx(2).unwrap().get("b"), Some(&Json::Null));
        assert_eq!(v.get("c").unwrap().as_bool(), Some(true));
    }

    #[test]
    fn rejeita_lixo_e_profundidade() {
        assert_eq!(
            Json::parse("{} x").unwrap_err().kind,
            JsonErrorKind::TrailingData
        );
        assert_eq!(
            Json::parse("[1,]").unwrap_err().kind,
            JsonErrorKind::UnexpectedByte
        );
        let deep = "[".repeat(MAX_DEPTH + 5);
        assert_eq!(
            Json::parse(&deep).unwrap_err().kind,
            JsonErrorKind::UnexpectedEnd
        );
    }

    #[test]
    fn unicode_e_roundtrip() {
        let v = Json::parse(r#""\u00e9\u4e2d""#).unwrap();
        assert_eq!(v.as_str(), Some("é中"));
        let original = Json::obj([
            ("s", Json::str("a\"b\\c\n")),
            ("n", Json::num(3.5)),
            ("a", Json::Arr(vec![Json::num(1.0), Json::Bool(false)])),
        ]);
        let text = original.to_text();
        assert_eq!(Json::parse(&text).unwrap(), original);
    }

    #[test]
    fn numero_inteiro_sem_ponto() {
        assert_eq!(Json::num(4.0).to_text(), "4");
        assert_eq!(Json::num(4.25).to_text(), "4.25");
        assert_eq!(Json::num(f64::NAN).to_text(), "null");
    }
}
