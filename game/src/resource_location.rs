use std::{ borrow::Cow };

const fn is_valid_char_u8(b: u8) -> bool {
    match b {
        b'0'..=b'9' | b'a'..=b'z' | b'_' | b'-' | b'.' => true,
        _ => false,
    }
}

const fn is_valid_char(b: char) -> bool {
    match b {
        '0'..='9' | 'a'..='z' | '_' | '-' | '.' => true,
        _ => false,
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ResourceLocation {
    string: Cow<'static, str>,
    colon: usize,
}

impl ResourceLocation {
    fn check_inner(text: &str) -> Option<usize> {
        let (namespace, path) = text.split_once(':')?;

        if namespace.is_empty() || namespace == ".." || path.is_empty() {
            return None;
        }

        if !namespace.chars().all(is_valid_char) {
            return None;
        }

        if !path.chars().all(|c| is_valid_char(c) || c == '/') {
            return None;
        }

        Some(namespace.len())
    }

    pub fn check(text: &str) -> bool {
        Self::check_inner(text).is_some()
    }

    pub fn new(string: impl Into<String>) -> Option<Self> {
        let string = string.into();
        let colon = Self::check_inner(&string)?;
        Some(Self { string: Cow::Owned(string), colon })
    }

    pub const fn new_const(str: &'static str) -> Self {
        assert!(str.is_ascii());
        let bytes = str.as_bytes();

        const fn is_allowed(b: u8) -> bool {
            match b {
                b'0'..=b'9' | b'a'..=b'z' | b'_' | b'-' | b'.' => true,
                _ => false,
            }
        }

        let mut colon_pos = None::<usize>;
        let mut i = 0usize;
        while i < bytes.len() {
            let c = bytes[i];
            if c == b':' {
                assert!(colon_pos.is_none());
                colon_pos = Some(i);
            }
            else {
                assert!(is_allowed(c) || (colon_pos.is_some() && c == b'/'));
            }
            i += 1;
        }
        let colon_pos = colon_pos.expect("Identifier must have a colon");
        assert!(colon_pos > 1, "Identifier must have non empty namespace");
        assert!(bytes.len() > colon_pos + 1, "Identifier must have non empty path");

        Self {
            string: Cow::Borrowed(str),
            colon: colon_pos,
        }
    }

    pub fn from_parts(namespace: impl AsRef<str>, path: impl AsRef<str>) -> Option<Self> {
        Self::new(format!("{}:{}", namespace.as_ref(), path.as_ref()))
    }

    pub fn into_inner(self) -> Cow<'static, str> {
        self.string
    }

    pub fn namespace(&self) -> &str {
        &self.string[..self.colon]
    }

    pub fn path(&self) -> &str {
        &self.string[self.colon+1..]
    }

    pub fn as_str(&self) -> &str {
        &self.string
    }
}

impl std::fmt::Debug for ResourceLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_str().fmt(f)
    }
}

impl Into<Cow<'static, str>> for ResourceLocation {
    fn into(self) -> Cow<'static, str> {
        self.string
    }
}

impl Into<String> for ResourceLocation {
    fn into(self) -> String {
        self.string.into()
    }
}
