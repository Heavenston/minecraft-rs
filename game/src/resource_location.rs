use std::{ borrow::Cow };

const fn is_valid_char(b: char) -> bool {
    matches!(b, '0'..='9' | 'a'..='z' | '_' | '-' | '.')
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ResourceLocation {
    string: Cow<'static, str>,
    colon: usize,
}

impl ResourceLocation {
    fn check_inner(text: &str) -> Result<Option<usize>, ()> {
        let (path, colon) = if let Some((namespace, path)) = text.split_once(':') {
            if namespace.is_empty() || namespace == ".." {
                return Err(());
            }

            if !namespace.chars().all(is_valid_char) {
                return Err(());
            }

            (path, Some(namespace.len()))
        }
        else {
            (text, None)
        };

        if path.is_empty() {
            return Err(());
        }

        if !path.chars().all(|c| is_valid_char(c) || c == '/') {
            return Err(());
        }

        Ok(colon)
    }

    pub fn check(text: &str) -> bool {
        Self::check_inner(text).is_ok()
    }

    pub fn new(string: impl Into<String>) -> Option<Self> {
        let mut string = string.into();
        let colon = Self::check_inner(&string).ok()?.unwrap_or_else(|| {
            string.insert_str(0, "minecraft:");
            9
        });
        Some(Self { string: Cow::Owned(string), colon })
    }

    pub const fn new_const(str: &'static str) -> Self {
        assert!(str.is_ascii());
        let bytes = str.as_bytes();

        let mut colon_pos = None::<usize>;
        let mut i = 0usize;
        while i < bytes.len() {
            let c = bytes[i];
            if c == b':' {
                assert!(colon_pos.is_none());
                colon_pos = Some(i);
            }
            else {
                let is_valid_char = {
                    match char::from_u32(c as u32) {
                        None => false,
                        Some(x) => is_valid_char(x),
                    }
                };
                assert!(is_valid_char || (colon_pos.is_some() && c == b'/'));
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

impl std::fmt::Display for ResourceLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_str().fmt(f)
    }
}

impl From<ResourceLocation> for Cow<'static, str> {
    fn from(val: ResourceLocation) -> Self {
        val.string
    }
}

impl From<ResourceLocation> for String {
    fn from(val: ResourceLocation) -> Self {
        val.string.into()
    }
}

impl<'de> serde::Deserialize<'de> for ResourceLocation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: serde::Deserializer<'de> {
        struct ResourceLocationVisitor;

        impl serde::de::Visitor<'_> for ResourceLocationVisitor {
            type Value = ResourceLocation;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(formatter, "a valid minecraft resource location")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where E: serde::de::Error,
            {
                ResourceLocation::new(v).ok_or_else(|| E::custom("invalid minecraft resource location"))
            }

            fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
            where E: serde::de::Error,
            {
                ResourceLocation::new(v).ok_or_else(|| E::custom("invalid minecraft resource location"))
            }
        }

        deserializer.deserialize_string(ResourceLocationVisitor)
    }
}
