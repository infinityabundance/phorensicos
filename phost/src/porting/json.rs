// porting/json.rs — a minimal JSON reader
//
// The court evidence is JSON, and the persistent port store must be *loaded* at
// runtime rather than re-derived, so the runtime has to read JSON. This is a
// deliberately small reader for exactly the shapes the evidence uses: objects,
// arrays, strings, numbers, booleans and null.
//
// It exists so the trusted core gains no parser dependency (no serde), and it is
// tested directly. Numbers keep their raw token, so nothing is lost reconstructing
// them.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// A parsed JSON value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Json {
    Null,
    Bool(bool),
    /// The raw number token, verbatim.
    Num(String),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    /// Parse one complete JSON document (trailing whitespace allowed).
    pub fn parse(text: &str) -> Result<Json, String> {
        Parser::new(text).document()
    }

    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Json::Num(n) => n.parse().ok(),
            _ => None,
        }
    }

    pub fn as_arr(&self) -> Option<&[Json]> {
        match self {
            Json::Arr(a) => Some(a),
            _ => None,
        }
    }

    /// A string field, or `None` if absent or not a string.
    pub fn str_at(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(|v| v.as_str())
    }

    /// A string field, or `""` when absent (convenient for optional provenance).
    pub fn str_or_empty(&self, key: &str) -> String {
        self.str_at(key).unwrap_or("").to_string()
    }

    pub fn i64_at(&self, key: &str) -> Option<i64> {
        self.get(key).and_then(|v| v.as_i64())
    }

    /// An array-of-strings field, or an empty vector.
    pub fn strings_at(&self, key: &str) -> Vec<String> {
        self.get(key)
            .and_then(|v| v.as_arr())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            b: text.as_bytes(),
            i: 0,
        }
    }

    fn document(mut self) -> Result<Json, String> {
        self.ws();
        let v = self.value()?;
        self.ws();
        if self.i != self.b.len() {
            return Err(format!("trailing input at byte {}", self.i));
        }
        Ok(v)
    }

    fn ws(&mut self) {
        while let Some(c) = self.peek() {
            if c == b' ' || c == b'\t' || c == b'\n' || c == b'\r' {
                self.i += 1;
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn here(&self) -> String {
        format!("at byte {}", self.i)
    }

    fn expect(&mut self, c: u8) -> Result<(), String> {
        if self.peek() == Some(c) {
            self.i += 1;
            Ok(())
        } else {
            Err(format!("expected '{}' {}", c as char, self.here()))
        }
    }

    fn literal(&mut self, s: &[u8]) -> Result<(), String> {
        if self.b.len() - self.i >= s.len() && &self.b[self.i..self.i + s.len()] == s {
            self.i += s.len();
            Ok(())
        } else {
            Err(format!("bad literal {}", self.here()))
        }
    }

    fn value(&mut self) -> Result<Json, String> {
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b't') => {
                self.literal(b"true")?;
                Ok(Json::Bool(true))
            }
            Some(b'f') => {
                self.literal(b"false")?;
                Ok(Json::Bool(false))
            }
            Some(b'n') => {
                self.literal(b"null")?;
                Ok(Json::Null)
            }
            Some(c) if c == b'-' || c.is_ascii_digit() => self.number(),
            _ => Err(format!("unexpected value {}", self.here())),
        }
    }

    fn object(&mut self) -> Result<Json, String> {
        self.expect(b'{')?;
        let mut fields: Vec<(String, Json)> = Vec::new();
        self.ws();
        if self.peek() == Some(b'}') {
            self.i += 1;
            return Ok(Json::Obj(fields));
        }
        loop {
            self.ws();
            let k = self.string()?;
            self.ws();
            self.expect(b':')?;
            self.ws();
            let v = self.value()?;
            fields.push((k, v));
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b'}') => {
                    self.i += 1;
                    break;
                }
                _ => return Err(format!("expected ',' or '}}' {}", self.here())),
            }
        }
        Ok(Json::Obj(fields))
    }

    fn array(&mut self) -> Result<Json, String> {
        self.expect(b'[')?;
        let mut items: Vec<Json> = Vec::new();
        self.ws();
        if self.peek() == Some(b']') {
            self.i += 1;
            return Ok(Json::Arr(items));
        }
        loop {
            self.ws();
            items.push(self.value()?);
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    break;
                }
                _ => return Err(format!("expected ',' or ']' {}", self.here())),
            }
        }
        Ok(Json::Arr(items))
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            let c = self.peek().ok_or_else(|| format!("unterminated string"))?;
            match c {
                b'"' => {
                    self.i += 1;
                    return Ok(out);
                }
                b'\\' => {
                    self.i += 1;
                    let e = self.peek().ok_or_else(|| format!("bad escape"))?;
                    self.i += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hi = self.hex4()?;
                            let ch = if (0xD800..0xDC00).contains(&hi) {
                                // A surrogate pair: expect a low surrogate next.
                                if self.peek() == Some(b'\\') {
                                    self.i += 1;
                                    self.expect(b'u')?;
                                    let lo = self.hex4()?;
                                    let cp = 0x10000
                                        + (((hi - 0xD800) as u32) << 10)
                                        + (lo - 0xDC00) as u32;
                                    char::from_u32(cp).unwrap_or('\u{fffd}')
                                } else {
                                    '\u{fffd}'
                                }
                            } else {
                                char::from_u32(hi as u32).unwrap_or('\u{fffd}')
                            };
                            out.push(ch);
                        }
                        _ => return Err(format!("bad escape '\\{}' {}", e as char, self.here())),
                    }
                }
                _ => {
                    // Copy one UTF-8 scalar verbatim.
                    let start = self.i;
                    self.i += 1;
                    while self.i < self.b.len() && (self.b[self.i] & 0xC0) == 0x80 {
                        self.i += 1;
                    }
                    match core::str::from_utf8(&self.b[start..self.i]) {
                        Ok(s) => out.push_str(s),
                        Err(_) => return Err(format!("invalid UTF-8 {}", self.here())),
                    }
                }
            }
        }
    }

    fn hex4(&mut self) -> Result<u16, String> {
        let mut v: u16 = 0;
        for _ in 0..4 {
            let c = self
                .peek()
                .ok_or_else(|| format!("bad \\u {}", self.here()))?;
            let d = match c {
                b'0'..=b'9' => c - b'0',
                b'a'..=b'f' => c - b'a' + 10,
                b'A'..=b'F' => c - b'A' + 10,
                _ => return Err(format!("bad hex digit {}", self.here())),
            };
            v = v * 16 + d as u16;
            self.i += 1;
        }
        Ok(v)
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.i;
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == b'.' || c == b'e' || c == b'E' || c == b'+' || c == b'-' {
                self.i += 1;
            } else {
                break;
            }
        }
        if self.i == start {
            return Err(format!("bad number {}", self.here()));
        }
        let token = core::str::from_utf8(&self.b[start..self.i])
            .map_err(|_| format!("bad number bytes {}", self.here()))?;
        Ok(Json::Num(token.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parses_objects_arrays_and_scalars() {
        let v = Json::parse(r#"{"a":1,"b":"x","c":[1,2,3],"d":true,"e":null,"f":{}}"#).unwrap();
        assert_eq!(v.i64_at("a"), Some(1));
        assert_eq!(v.str_at("b"), Some("x"));
        assert_eq!(v.strings_at("c").len(), 0); // numbers, not strings
        assert_eq!(v.get("d"), Some(&Json::Bool(true)));
        assert_eq!(v.get("e"), Some(&Json::Null));
        assert!(matches!(v.get("f"), Some(Json::Obj(f)) if f.is_empty()));
    }

    #[test]
    fn test_parses_strings_and_escapes() {
        let v = Json::parse(r#"{"s":"a\"b\\c\nd","u":"\u0041\u00e9","arr":["x","y"]}"#).unwrap();
        assert_eq!(v.str_at("s"), Some("a\"b\\c\nd"));
        assert_eq!(v.str_at("u"), Some("Aé"));
        assert_eq!(v.strings_at("arr"), alloc::vec!["x", "y"]);
    }

    #[test]
    fn test_parses_the_evidence_shapes() {
        // A composition verdict as the store reader sees it.
        let doc = Json::parse(
            r#"{"schema":"s","stages":["libc:a","libc:b"],"chain_hash":"ab","cases_run":7,"mismatches":[]}"#,
        )
        .unwrap();
        assert_eq!(doc.str_at("chain_hash"), Some("ab"));
        assert_eq!(doc.strings_at("stages"), alloc::vec!["libc:a", "libc:b"]);
        assert_eq!(doc.i64_at("cases_run"), Some(7));
        assert_eq!(
            doc.get("mismatches")
                .and_then(|m| m.as_arr())
                .map(|a| a.len()),
            Some(0)
        );
    }

    #[test]
    fn test_rejects_malformed_input() {
        assert!(Json::parse("").is_err());
        assert!(Json::parse("{").is_err());
        assert!(Json::parse("{}x").is_err());
        assert!(Json::parse(r#"{"a":}"#).is_err());
        assert!(Json::parse(r#"{"a" 1}"#).is_err());
        assert!(Json::parse(r#"["a",]"#).is_err());
    }

    #[test]
    fn test_empty_containers_and_whitespace() {
        assert_eq!(Json::parse("  { }  ").unwrap(), Json::Obj(Vec::new()));
        assert_eq!(Json::parse("[]").unwrap(), Json::Arr(Vec::new()));
        assert_eq!(
            Json::parse(" [ 1 , 2 ] ").unwrap().as_arr().unwrap().len(),
            2
        );
    }
}
