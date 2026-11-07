//! The little JSON the crate needs: writing the options object, and reading the PDF/A report's two lists of strings.
//! Written by hand so the crate has no dependencies.

use crate::error::{Error, ErrorKind, Result};

/// Builds a flat JSON object, one option at a time.
#[derive(Default)]
pub(crate) struct Object {
    out: String,
}

impl Object {
    fn key(&mut self, name: &str) {
        self.out.push(if self.out.is_empty() { '{' } else { ',' });
        string(&mut self.out, name);
        self.out.push(':');
    }

    pub(crate) fn str(&mut self, name: &str, value: &str) {
        self.key(name);
        string(&mut self.out, value);
    }

    pub(crate) fn bool(&mut self, name: &str, value: bool) {
        self.key(name);
        self.out.push_str(if value { "true" } else { "false" });
    }

    pub(crate) fn int(&mut self, name: &str, value: i64) {
        self.key(name);
        self.out.push_str(&value.to_string());
    }

    pub(crate) fn num(&mut self, name: &str, value: f64) -> Result<()> {
        if !value.is_finite() {
            return Err(Error::argument(format!(
                "{name} must be a finite number, not {value}"
            )));
        }
        self.key(name);
        // Display never uses an exponent and always uses a dot, so this is valid JSON in any locale.
        self.out.push_str(&value.to_string());
        Ok(())
    }

    pub(crate) fn nums(&mut self, name: &str, values: &[f64]) -> Result<()> {
        if let Some(v) = values.iter().find(|v| !v.is_finite()) {
            return Err(Error::argument(format!(
                "{name} must be finite numbers, not {v}"
            )));
        }
        self.key(name);
        self.out.push('[');
        self.out.push_str(
            &values
                .iter()
                .map(f64::to_string)
                .collect::<Vec<_>>()
                .join(","),
        );
        self.out.push(']');
        Ok(())
    }

    pub(crate) fn strs(&mut self, name: &str, values: &[String]) {
        self.key(name);
        self.out.push('[');
        for (i, v) in values.iter().enumerate() {
            if i > 0 {
                self.out.push(',');
            }
            string(&mut self.out, v);
        }
        self.out.push(']');
    }

    pub(crate) fn finish(mut self) -> String {
        if self.out.is_empty() {
            self.out.push('{');
        }
        self.out.push('}');
        self.out
    }
}

fn string(out: &mut String, s: &str) {
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

/// Reads `{"name": ["string", ...], ...}`, the shape of the PDF/A report, and returns the list under `name`.
pub(crate) fn string_list(json: &str, name: &str) -> Result<Vec<String>> {
    let mut p = Parser {
        s: json.as_bytes(),
        at: 0,
    };
    p.object_of_lists(name).ok_or_else(|| {
        Error::new(
            ErrorKind::Internal,
            "the native library returned a malformed report",
        )
    })
}

struct Parser<'a> {
    s: &'a [u8],
    at: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.s.get(self.at).is_some_and(|b| b.is_ascii_whitespace()) {
            self.at += 1;
        }
    }

    fn eat(&mut self, b: u8) -> Option<()> {
        self.ws();
        (self.s.get(self.at) == Some(&b)).then(|| self.at += 1)
    }

    fn object_of_lists(&mut self, wanted: &str) -> Option<Vec<String>> {
        self.eat(b'{')?;
        let mut found = None;
        if self.eat(b'}').is_some() {
            return found;
        }
        loop {
            let key = self.string()?;
            self.eat(b':')?;
            let list = self.list()?;
            if key == wanted {
                found = Some(list);
            }
            if self.eat(b',').is_none() {
                self.eat(b'}')?;
                return found;
            }
        }
    }

    fn list(&mut self) -> Option<Vec<String>> {
        self.eat(b'[')?;
        let mut items = Vec::new();
        if self.eat(b']').is_some() {
            return Some(items);
        }
        loop {
            items.push(self.string()?);
            if self.eat(b',').is_none() {
                self.eat(b']')?;
                return Some(items);
            }
        }
    }

    fn hex4(&mut self) -> Option<u32> {
        let digits = std::str::from_utf8(self.s.get(self.at..self.at + 4)?).ok()?;
        self.at += 4;
        u32::from_str_radix(digits, 16).ok()
    }

    fn string(&mut self) -> Option<String> {
        self.eat(b'"')?;
        let mut bytes = Vec::new();
        loop {
            let b = *self.s.get(self.at)?;
            self.at += 1;
            match b {
                b'"' => return String::from_utf8(bytes).ok(),
                b'\\' => {
                    let e = *self.s.get(self.at)?;
                    self.at += 1;
                    let c = match e {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let mut code = self.hex4()?;
                            if (0xd800..0xdc00).contains(&code)
                                && self.s.get(self.at..self.at + 2) == Some(b"\\u")
                            {
                                self.at += 2;
                                let low = self.hex4()?;
                                code = 0x10000
                                    + ((code - 0xd800) << 10)
                                    + (low.checked_sub(0xdc00)? & 0x3ff);
                            }
                            char::from_u32(code).unwrap_or('\u{fffd}')
                        }
                        _ => return None,
                    };
                    let mut buf = [0; 4];
                    bytes.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
                _ => bytes.push(b),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_escaped_options() {
        let mut o = Object::default();
        o.str("title", "a \"quote\"\n\u{1}é");
        o.bool("compress", false);
        o.num("scale", 0.5).unwrap();
        o.nums("margin", &[1.0, 2.5, 3.0, 4.0]).unwrap();
        assert_eq!(
            o.finish(),
            r#"{"title":"a \"quote\"\n\u0001é","compress":false,"scale":0.5,"margin":[1,2.5,3,4]}"#
        );
        assert_eq!(Object::default().finish(), "{}");
        assert!(Object::default().num("scale", f64::NAN).is_err());
    }

    #[test]
    fn reads_the_report() {
        let json =
            r#"{"changes":["Embedded \u0022Arial\u0022","\u00E6\uD83D\uDE00 \\ /"],"problems":[]}"#;
        assert_eq!(
            string_list(json, "changes").unwrap(),
            vec!["Embedded \"Arial\"", "\u{e6}\u{1f600} \\ /"]
        );
        assert!(string_list(json, "problems").unwrap().is_empty());
        assert!(string_list("{\"changes\":[1]}", "changes").is_err());
    }
}
