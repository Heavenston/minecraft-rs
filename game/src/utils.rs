use glam::{ISizeVec3, USizeVec3, Vec3, Vec3Swizzles};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vec3Range(pub USizeVec3, pub USizeVec3);

impl IntoIterator for Vec3Range {
    type Item = USizeVec3;
    type IntoIter = Vec3RangeIter;

    fn into_iter(self) -> Self::IntoIter {
        Vec3RangeIter {
            range: self,
            current: self.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vec3RangeIter {
    range: Vec3Range,
    current: USizeVec3,
}

impl Iterator for Vec3RangeIter {
    type Item = USizeVec3;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current.x < self.range.1.x {
            let val = self.current;
            self.current.x += 1;
            return Some(val);
        }
        self.current.x = self.range.0.x;
        self.current.y += 1;
        if self.current.y < self.range.1.y {
            return self.next();
        }
        self.current.y = self.range.0.y;
        self.current.z += 1;
        if self.current.z < self.range.1.z {
            return self.next();
        }
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ISizeVec3Range(pub ISizeVec3, pub ISizeVec3);

impl IntoIterator for ISizeVec3Range {
    type Item = ISizeVec3;
    type IntoIter = ISizeVec3RangeIter;

    fn into_iter(self) -> Self::IntoIter {
        ISizeVec3RangeIter {
            range: self,
            current: self.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ISizeVec3RangeIter {
    range: ISizeVec3Range,
    current: ISizeVec3,
}

impl Iterator for ISizeVec3RangeIter {
    type Item = ISizeVec3;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current.x < self.range.1.x {
            let val = self.current;
            self.current.x += 1;
            return Some(val);
        }
        self.current.x = self.range.0.x;
        self.current.y += 1;
        if self.current.y < self.range.1.y {
            return self.next();
        }
        self.current.y = self.range.0.y;
        self.current.z += 1;
        if self.current.z < self.range.1.z {
            return self.next();
        }
        None
    }
}

#[test]
fn test_vec3_iter() {
    use itertools::Itertools as _;

    assert_eq!(
        Vec3Range(USizeVec3::new(0, 4, 3), USizeVec3::new(5, 5, 5)).into_iter().map(|p| p.to_array()).collect_vec().as_slice(), &[
        [0, 4, 3], [1, 4, 3], [2, 4, 3], [3, 4, 3], [4, 4, 3],
        [0, 4, 4], [1, 4, 4], [2, 4, 4], [3, 4, 4], [4, 4, 4],
    ][..]);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, enum_map::Enum, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
#[enumflags2::bitflags]
#[repr(u8)]
#[expect(clippy::use_self, reason = "bitflags macro generated")]
pub enum Axis {
    X, Y, Z,
}

impl Axis {
    pub const fn pos(self) -> CardinalDirection {
        match self {
            Self::X => CardinalDirection::PosX,
            Self::Y => CardinalDirection::PosY,
            Self::Z => CardinalDirection::PosZ,
        }
    }

    pub const fn neg(self) -> CardinalDirection {
        self.pos().opposit()
    }

    pub const fn as_usizevec3(self) -> USizeVec3 {
        match self {
            Self::X => USizeVec3::X,
            Self::Y => USizeVec3::Y,
            Self::Z => USizeVec3::Z,
        }
    }

    pub const fn as_vec3(self) -> Vec3 {
        match self {
            Self::X => Vec3::X,
            Self::Y => Vec3::Y,
            Self::Z => Vec3::Z,
        }
    }
}

macro_rules! impl_index_axis {
    () => {};
    ($name:ty, $ty:ty$(;$($rest:tt)*)?) => {
        const impl std::ops::Index<Axis> for $name {
            type Output = $ty;

            fn index(&self, index: Axis) -> &Self::Output {
                match index {
                    Axis::X => &self.x,
                    Axis::Y => &self.y,
                    Axis::Z => &self.z,
                }
            }
        }

        const impl std::ops::IndexMut<Axis> for $name {
            fn index_mut(&mut self, index: Axis) -> &mut Self::Output {
                match index {
                    Axis::X => &mut self.x,
                    Axis::Y => &mut self.y,
                    Axis::Z => &mut self.z,
                }
            }
        }

        const impl std::ops::Sub<Axis> for $name {
            type Output = <Self as Vec3Swizzles>::Vec2;

            fn sub(self, rhs: Axis) -> Self::Output {
                match rhs {
                    Axis::X => <Self as Vec3Swizzles>::Vec2::new(self.y, self.z),
                    Axis::Y => <Self as Vec3Swizzles>::Vec2::new(self.x, self.z),
                    Axis::Z => <Self as Vec3Swizzles>::Vec2::new(self.x, self.y),
                }
            }
        }

        impl_index_axis!($($($rest)*)?);
    };
}
impl_index_axis!(
    glam::I8Vec3, i8 ; glam::I16Vec3, i16; glam::IVec3, i32 ; glam::I64Vec3, i64;
    glam::U8Vec3, u8 ; glam::U16Vec3, u16; glam::UVec3, u32 ; glam::U64Vec3, u64;
    glam::Vec3  , f32; glam::DVec3  , f64;
    glam::USizeVec3, usize; glam::ISizeVec3, isize;
);

#[derive(serde::Deserialize, Debug, Clone, Copy, enum_map::Enum)]
#[enumflags2::bitflags]
#[repr(u8)]
#[expect(clippy::use_self, reason = "bitflags macro generated")]
pub enum CardinalDirection {
    #[serde(rename = "east")]
    PosX,
    #[serde(rename = "west")]
    NegX,
    #[serde(rename = "up")]
    PosY,
    #[serde(rename = "down")]
    NegY,
    #[serde(rename = "south")]
    PosZ,
    #[serde(rename = "north")]
    NegZ,
}

impl CardinalDirection {
    pub const VALUES: [Self; 6] = [
        Self::PosX,
        Self::NegX,
        Self::PosY,
        Self::NegY,
        Self::PosZ,
        Self::NegZ,
    ];

    pub const fn opposit(self) -> Self {
        match self {
            Self::PosX => Self::NegX,
            Self::NegX => Self::PosX,
            Self::PosY => Self::NegY,
            Self::NegY => Self::PosY,
            Self::PosZ => Self::NegZ,
            Self::NegZ => Self::PosZ,
        }
    }

    pub const fn as_isizevec3(self) -> ISizeVec3 {
        match self {
            Self::PosX => ISizeVec3::X,
            Self::NegX => ISizeVec3::NEG_X,
            Self::PosY => ISizeVec3::Y,
            Self::NegY => ISizeVec3::NEG_Y,
            Self::PosZ => ISizeVec3::Z,
            Self::NegZ => ISizeVec3::NEG_Z,
        }
    }

    pub const fn axis(self) -> Axis {
        match self {
            Self::PosX | Self::NegX => Axis::X,
            Self::PosY | Self::NegY => Axis::Y,
            Self::PosZ | Self::NegZ => Axis::Z,
        }
    }

    pub const fn abs(self) -> Self {
        self.axis().pos()
    }

    pub const fn is_positive(self) -> bool { self.abs() == self }
    pub const fn is_negative(self) -> bool { !self.is_positive() }
}

const impl From<Axis> for CardinalDirection {
    fn from(val: Axis) -> Self {
        match val {
            Axis::X => Self::PosX,
            Axis::Y => Self::PosY,
            Axis::Z => Self::PosZ,
        }
    }
}

const impl std::ops::Neg for CardinalDirection {
    type Output = Self;

    fn neg(self) -> Self::Output {
        self.opposit()
    }
}

const impl std::str::FromStr for CardinalDirection {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "posx" | "+x" | "east"  => Self::PosX,
            "negx" | "-x" | "west"  => Self::NegX,
            "posy" | "+y" | "up"    => Self::PosY,
            "negy" | "-y" | "down"  => Self::NegY,
            "posz" | "+z" | "south" => Self::PosZ,
            "negz" | "-z" | "north" => Self::NegZ,

            _ => return Err(()),
        })
    }
}

const impl std::ops::Add<CardinalDirection> for USizeVec3 {
    type Output = Self;
    fn add(self, rhs: CardinalDirection) -> Self::Output {
        if cfg!(debug_assertions) {
            self.checked_add_signed(rhs.as_isizevec3()).expect("arithetic overflow")
        }
        else {
            self.wrapping_add_signed(rhs.as_isizevec3())
        }
    }
}

const impl std::ops::Add<CardinalDirection> for ISizeVec3 {
    type Output = Self;
    fn add(self, rhs: CardinalDirection) -> Self::Output {
        if cfg!(debug_assertions) {
            self.checked_add(rhs.as_isizevec3()).expect("arithetic overflow")
        }
        else {
            self.wrapping_add(rhs.as_isizevec3())
        }
    }
}

const impl PartialEq for CardinalDirection {
    fn eq(&self, other: &Self) -> bool {
        match self {
            Self::PosX => matches!(other, Self::PosX),
            Self::NegX => matches!(other, Self::NegX),
            Self::PosY => matches!(other, Self::PosY),
            Self::NegY => matches!(other, Self::NegY),
            Self::PosZ => matches!(other, Self::PosZ),
            Self::NegZ => matches!(other, Self::NegZ),
        }
    }
}
const impl Eq for CardinalDirection {}

impl std::hash::Hash for CardinalDirection {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
    }
}
