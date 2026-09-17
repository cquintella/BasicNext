use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Value {
    Null,
    Boolean(bool),
    Number(f64),
    String(String),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
}

const MAX_DEPTH: usize = 64;
const MAX_BYTES: usize = 8 * 1024 * 1024;

pub(crate) fn parse(input: &str) -> Result<Value, &'static str> {
    if input.len() > MAX_BYTES {
        return Err("JSON input exceeds 8 MiB");
    }
    let mut parser = Parser {
        input: input.as_bytes(),
        offset: 0,
    };
    let value = parser.value(0)?;
    parser.ws();
    if parser.offset != parser.input.len() {
        return Err("trailing JSON input");
    }
    Ok(value)
}

pub(crate) fn stringify(value: &Value) -> Result<String, &'static str> {
    let mut output = String::new();
    write_value(value, &mut output, 0)?;
    if output.len() > MAX_BYTES {
        return Err("JSON output exceeds 8 MiB");
    }
    Ok(output)
}

struct Parser<'a> {
    input: &'a [u8],
    offset: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self
            .input
            .get(self.offset)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.offset += 1;
        }
    }
    fn value(&mut self, depth: usize) -> Result<Value, &'static str> {
        if depth > MAX_DEPTH {
            return Err("JSON nesting exceeds 64 levels");
        }
        self.ws();
        match self.input.get(self.offset).copied() {
            Some(b'n') => self.literal(b"null", Value::Null),
            Some(b't') => self.literal(b"true", Value::Boolean(true)),
            Some(b'f') => self.literal(b"false", Value::Boolean(false)),
            Some(b'"') => self.string().map(Value::String),
            Some(b'[') => self.array(depth),
            Some(b'{') => self.object(depth),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err("invalid JSON value"),
        }
    }
    fn literal(&mut self, literal: &[u8], value: Value) -> Result<Value, &'static str> {
        if self.input.get(self.offset..self.offset + literal.len()) == Some(literal) {
            self.offset += literal.len();
            Ok(value)
        } else {
            Err("invalid JSON literal")
        }
    }
    fn string(&mut self) -> Result<String, &'static str> {
        self.offset += 1;
        let mut out = String::new();
        while let Some(byte) = self.input.get(self.offset).copied() {
            self.offset += 1;
            match byte {
                b'"' => return Ok(out),
                b'\\' => {
                    let escape = self
                        .input
                        .get(self.offset)
                        .copied()
                        .ok_or("unterminated JSON escape")?;
                    self.offset += 1;
                    if escape == b'u' {
                        let first = self.unicode_escape()?;
                        let code = if (0xD800..=0xDBFF).contains(&first) {
                            if self.input.get(self.offset..self.offset + 2) != Some(b"\\u") {
                                return Err("unpaired JSON high surrogate");
                            }
                            self.offset += 2;
                            let second = self.unicode_escape()?;
                            if !(0xDC00..=0xDFFF).contains(&second) {
                                return Err("invalid JSON surrogate pair");
                            }
                            0x1_0000 + ((u32::from(first) - 0xD800) << 10) + u32::from(second)
                                - 0xDC00
                        } else if (0xDC00..=0xDFFF).contains(&first) {
                            return Err("unpaired JSON low surrogate");
                        } else {
                            u32::from(first)
                        };
                        out.push(char::from_u32(code).ok_or("invalid JSON code point")?);
                        continue;
                    }
                    out.push(match escape {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        _ => return Err("unsupported JSON escape"),
                    });
                }
                0..=0x1f => return Err("JSON string contains a control character"),
                _ => {
                    let start = self.offset - 1;
                    while self.input.get(self.offset).is_some_and(|b| *b >= 0x80) {
                        self.offset += 1;
                    }
                    out.push_str(
                        std::str::from_utf8(&self.input[start..self.offset])
                            .map_err(|_| "invalid UTF-8 in JSON string")?,
                    );
                }
            }
        }
        Err("unterminated JSON string")
    }
    fn unicode_escape(&mut self) -> Result<u16, &'static str> {
        let end = self.offset.checked_add(4).ok_or("invalid JSON escape")?;
        let digits = self
            .input
            .get(self.offset..end)
            .ok_or("invalid JSON escape")?;
        let code = digits
            .iter()
            .try_fold(0_u16, |value, digit| {
                value.checked_mul(16)?.checked_add(u16::from(hex(*digit)?))
            })
            .ok_or("invalid JSON escape")?;
        self.offset = end;
        Ok(code)
    }
    fn array(&mut self, depth: usize) -> Result<Value, &'static str> {
        self.offset += 1;
        self.ws();
        let mut values = Vec::new();
        if self.input.get(self.offset) == Some(&b']') {
            self.offset += 1;
            return Ok(Value::Array(values));
        }
        loop {
            values.push(self.value(depth + 1)?);
            self.ws();
            match self.input.get(self.offset).copied() {
                Some(b',') => {
                    self.offset += 1;
                }
                Some(b']') => {
                    self.offset += 1;
                    return Ok(Value::Array(values));
                }
                _ => return Err("invalid JSON array"),
            }
        }
    }
    fn object(&mut self, depth: usize) -> Result<Value, &'static str> {
        self.offset += 1;
        self.ws();
        let mut values = BTreeMap::new();
        if self.input.get(self.offset) == Some(&b'}') {
            self.offset += 1;
            return Ok(Value::Object(values));
        }
        loop {
            self.ws();
            let key = self.string()?;
            self.ws();
            if self.input.get(self.offset) != Some(&b':') {
                return Err("invalid JSON object");
            }
            self.offset += 1;
            let value = self.value(depth + 1)?;
            if values.insert(key, value).is_some() {
                return Err("duplicate JSON object key");
            }
            self.ws();
            match self.input.get(self.offset).copied() {
                Some(b',') => self.offset += 1,
                Some(b'}') => {
                    self.offset += 1;
                    return Ok(Value::Object(values));
                }
                _ => return Err("invalid JSON object"),
            }
        }
    }
    fn number(&mut self) -> Result<Value, &'static str> {
        let start = self.offset;
        while self
            .input
            .get(self.offset)
            .is_some_and(|b| b.is_ascii_digit() || matches!(b, b'-' | b'+' | b'.' | b'e' | b'E'))
        {
            self.offset += 1;
        }
        let text = std::str::from_utf8(&self.input[start..self.offset])
            .map_err(|_| "invalid JSON number")?;
        let number = text.parse::<f64>().map_err(|_| "invalid JSON number")?;
        if number.is_finite() {
            Ok(Value::Number(number))
        } else {
            Err("JSON number is not finite")
        }
    }
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn write_value(value: &Value, output: &mut String, depth: usize) -> Result<(), &'static str> {
    if depth > MAX_DEPTH {
        return Err("JSON nesting exceeds 64 levels");
    }
    match value {
        Value::Null => output.push_str("null"),
        Value::Boolean(value) => output.push_str(if *value { "true" } else { "false" }),
        Value::Number(value) if value.is_finite() => output.push_str(&value.to_string()),
        Value::Number(_) => return Err("JSON number is not finite"),
        Value::String(value) => {
            output.push('"');
            for ch in value.chars() {
                match ch {
                    '"' => output.push_str("\\\""),
                    '\\' => output.push_str("\\\\"),
                    '\n' => output.push_str("\\n"),
                    '\r' => output.push_str("\\r"),
                    '\t' => output.push_str("\\t"),
                    ch if ch.is_control() => {
                        return Err("JSON string contains a control character");
                    }
                    ch => output.push(ch),
                }
            }
            output.push('"');
        }
        Value::Array(values) => {
            output.push('[');
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                write_value(value, output, depth + 1)?;
            }
            output.push(']');
        }
        Value::Object(values) => {
            output.push('{');
            for (index, (key, value)) in values.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                write_value(&Value::String(key.clone()), output, depth + 1)?;
                output.push(':');
                write_value(value, output, depth + 1)?;
            }
            output.push('}');
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Value, parse, stringify};
    #[test]
    fn round_trips_bounded_json() {
        let value = parse(r#"{"ok":true,"items":[1,null]}"#).unwrap();
        assert_eq!(parse(&stringify(&value).unwrap()), Ok(value));
    }
    #[test]
    fn rejects_duplicate_keys_and_trailing_data() {
        assert!(parse(r#"{"x":1,"x":2}"#).is_err());
        assert!(parse("null false").is_err());
    }
    #[test]
    fn decodes_unicode_surrogate_pairs() {
        assert_eq!(parse(r#""\uD83D\uDE00""#), Ok(Value::String("😀".into())));
        assert!(parse(r#""\uD83D""#).is_err());
    }
}
