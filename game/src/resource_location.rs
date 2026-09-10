use std::{collections::HashMap, hash::{BuildHasherDefault, Hasher}};

use ustr::Ustr;

const fn is_valid_char(b: char) -> bool {
    matches!(b, '0'..='9' | 'a'..='z' | '_' | '-' | '.')
}

macro_rules! location {
    ($e: expr) => {{
        ::static_assertions::const_assert!(crate::resource_location::ResourceLocation::check($e));
        crate::resource_location::ResourceLocation::new($e).expect("checked at compile time")
    }};
}
pub(crate) use location;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResourceLocation {
    inner: Ustr,
}

impl ResourceLocation {
    pub const fn check(str: &str) -> bool {
        if !str.is_ascii() { return false }
        let bytes = str.as_bytes();

        let mut passed_namespace = false;
        let mut colon_pos = None::<usize>;
        let mut i = 0usize;
        while i < bytes.len() {
            let c = bytes[i];
            if !passed_namespace && colon_pos.is_none() && c == b':' {
                passed_namespace = true;
                colon_pos = Some(i);
            }
            else {
                let is_valid_char = match char::from_u32(c as u32) {
                    None => false,
                    Some(x) => is_valid_char(x),
                };
                if !is_valid_char {
                    if c == b'/' {
                        passed_namespace = true;
                    }
                    else {
                        return false;
                    }
                }
            }
            i += 1;
        }
        match colon_pos {
            Some(idx) =>
                // Namespace non empty
                idx != 0 &&
                // Path non empty
                (bytes.len() > idx + 1) &&
                // Namespace != ..
                (&bytes[..idx] != b".."),
            // Default namespace and empty path
            None => !bytes.is_empty(),
        }
    }
    
    pub fn new(str: &str) -> Option<Self> {
        Self::check(str).then(|| Self { inner: Ustr::from(str) })
    }

    pub fn namespace(&self) -> &str {
        const DEFAULT_NAMESPACE: &str = "minecraft";
        self.inner.split_once(':')
            .map_or(DEFAULT_NAMESPACE, |(namespace,_)| namespace)
    }

    pub fn path(&self) -> &str {
        self.inner.split_once(':')
            .map_or(&self.inner, |(_,path)| path)
    }

    pub fn as_str(&self) -> &str {
        &self.inner
    }
}

#[test]
fn test_resource_location_check() {
    use ResourceLocation as RL;
    assert!(RL::check("minecraft:basic"));
    assert!(RL::check("m:basic"));
    assert!(RL::check("m:basic/with/path"));
    assert!(RL::check("no_namespace"));
    assert!(RL::check("no_namespace/with/path"));
    assert!(!RL::check(""));
    assert!(!RL::check(":"));
    assert!(!RL::check("empty_path:"));
    assert!(!RL::check(":empty_namespace"));
    assert!(!RL::check("a/in:namespace"));
    assert!(!RL::check("multiple:colons:is_forbiden"));
    assert!(RL::check("a_single:colon_is_allowed"));

    assert!(!RL::check("..:is_not_a_valid_namespace"));
    assert!(RL::check("a..:is_valid"));
    assert!(RL::check("..a:is_valid"));
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

impl From<ResourceLocation> for &str {
    fn from(val: ResourceLocation) -> Self {
        val.inner.as_str()
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
        }

        deserializer.deserialize_str(ResourceLocationVisitor)
    }
}

#[derive(Default)]
pub struct PassThroughHasher {
    hash: u64,
}
impl Hasher for PassThroughHasher {
    fn finish(&self) -> u64 {
        self.hash
    }

    fn write(&mut self, bytes: &[u8]) {
        if let Ok(bytes) = <[u8; 8]>::try_from(bytes) {
            self.hash = u64::from_ne_bytes(bytes);
        }
        else {
            panic!("hasher called with wrong type")
        }
    }
}

/// [`std::collections::HashMap`] with custom hasher for [`ResourceLocation`] that does like [`ustr::UstrMap`].
pub type ResourceLocationMap<V> = HashMap<ResourceLocation, V, BuildHasherDefault<PassThroughHasher>>;
pub type ResourceLocationOrderSet<V> = ordermap::set::OrderSet<V, BuildHasherDefault<PassThroughHasher>>;
