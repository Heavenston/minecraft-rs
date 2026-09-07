use std::{borrow::Cow, fmt::Display, ops::Deref};

#[derive(Debug, Clone)]
pub enum Label<'a> {
    TypeName(&'static str),
    Other(Cow<'a, str>),
}

impl Label<'_> {
    pub fn as_str(&self) -> &str {
        match self {
            Label::TypeName(type_name) => type_name,
            Label::Other(cow) => cow,
        }
    }

    pub fn to_static(&self) -> Label<'static> {
        match self {
            Self::TypeName(tn)                   => Label::TypeName(tn),
            Self::Other(Cow::Borrowed(borrowed)) => Label::Other(Cow::Owned(borrowed.to_string())),
            Self::Other(Cow::Owned(owned))       => Label::Other(Cow::Owned(owned.clone())),
        }
    }

    pub fn into_static(self) -> Label<'static> {
        match self {
            Self::TypeName(tn)                   => Label::TypeName(tn),
            Self::Other(Cow::Borrowed(borrowed)) => Label::Other(Cow::Owned(borrowed.to_string())),
            Self::Other(Cow::Owned(owned))       => Label::Other(Cow::Owned(owned)),
        }
    }
}

impl PartialEq for Label<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}
impl Eq for Label<'_> {}

impl PartialOrd for Label<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Label<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl Deref for Label<'_> {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl Display for Label<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
