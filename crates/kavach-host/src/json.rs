//! Minimal JSON writer. There is no serde here, by design.

/// Escape a string for embedding in a JSON document.
pub fn escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 8);
    for ch in input.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Builder for flat JSON objects and arrays.
#[derive(Default)]
pub struct Json {
    buf: String,
}

impl Json {
    pub fn object() -> Self {
        Json { buf: String::from("{") }
    }

    fn comma(&mut self) {
        let last = self.buf.chars().last().unwrap_or('{');
        if last != '{' {
            self.buf.push(',');
        }
    }

    pub fn str(mut self, key: &str, value: &str) -> Self {
        self.comma();
        self.buf.push_str(&format!("\"{}\":\"{}\"", escape(key), escape(value)));
        self
    }

    pub fn raw(mut self, key: &str, value: &str) -> Self {
        self.comma();
        self.buf.push_str(&format!("\"{}\":{}", escape(key), value));
        self
    }

    pub fn num(mut self, key: &str, value: i64) -> Self {
        self.comma();
        self.buf.push_str(&format!("\"{}\":{}", escape(key), value));
        self
    }

    pub fn uint(mut self, key: &str, value: u64) -> Self {
        self.comma();
        self.buf.push_str(&format!("\"{}\":{}", escape(key), value));
        self
    }

    pub fn bool(mut self, key: &str, value: bool) -> Self {
        self.comma();
        self.buf.push_str(&format!("\"{}\":{}", escape(key), value));
        self
    }

    pub fn null(mut self, key: &str) -> Self {
        self.comma();
        self.buf.push_str(&format!("\"{}\":null", escape(key)));
        self
    }

    pub fn strings(mut self, key: &str, values: &[String]) -> Self {
        self.comma();
        let items: Vec<String> = values.iter().map(|v| format!("\"{}\"", escape(v))).collect();
        self.buf.push_str(&format!("\"{}\":[{}]", escape(key), items.join(",")));
        self
    }

    pub fn finish(mut self) -> String {
        self.buf.push('}');
        self.buf
    }
}

/// `{"ok":true,...}` envelope.
pub fn ok(payload: Json) -> String {
    let inner = payload.finish();
    let body = &inner[1..inner.len() - 1];
    if body.is_empty() {
        String::from("{\"ok\":true}")
    } else {
        format!("{{\"ok\":true,{}}}", body)
    }
}

/// `{"ok":false,"error":...,"kind":...}` envelope.
pub fn err(message: &str, kind: &str) -> String {
    Json::object()
        .bool("ok", false)
        .str("error", message)
        .str("kind", kind)
        .finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_control_characters() {
        assert_eq!(escape("a\"b\\c\nd\te\u{1}"), "a\\\"b\\\\c\\nd\\te\\u0001");
    }

    #[test]
    fn builds_objects_and_arrays() {
        let json = Json::object().str("n", "x").num("c", 3).bool("b", true).strings("l", &["a".into()]).finish();
        assert_eq!(json, "{\"n\":\"x\",\"c\":3,\"b\":true,\"l\":[\"a\"]}");
    }

    #[test]
    fn envelopes_are_well_formed() {
        assert_eq!(ok(Json::object().str("v", "1")), "{\"ok\":true,\"v\":\"1\"}");
        assert_eq!(ok(Json::object()), "{\"ok\":true}");
        assert_eq!(err("boom", "io"), "{\"ok\":false,\"error\":\"boom\",\"kind\":\"io\"}");
    }
}
