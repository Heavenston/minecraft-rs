use glam::{DVec2, DVec3, ISizeVec3, USizeVec3, Vec2, Vec3, Vec3Swizzles};

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

/// Defines clockwise rotation by increments of 90 degrees.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, serde_repr::Deserialize_repr)]
#[repr(u16)]
pub enum GridAngle {
    #[default]
    Zero = 0,
    Ninety = 90,
    OneEighty = 180,
    TwoSeventy = 270,
}

impl GridAngle {
    pub const VALUES: [Self; 4] = [
        Self::Zero, Self::Ninety, Self::OneEighty, Self::TwoSeventy,
    ];

    pub const fn to_radians_f32(self) -> f32 {
        match self {
            Self::Zero => 0.,
            Self::Ninety => std::f32::consts::FRAC_PI_2,
            Self::OneEighty => std::f32::consts::PI,
            Self::TwoSeventy => const { std::f32::consts::PI + std::f32::consts::FRAC_PI_2 },
        }
    }

    pub const fn to_radians_f64(self) -> f64 {
        match self {
            Self::Zero => 0.,
            Self::Ninety => std::f64::consts::FRAC_PI_2,
            Self::OneEighty => std::f64::consts::PI,
            Self::TwoSeventy => const { std::f64::consts::PI + std::f64::consts::FRAC_PI_2 },
        }
    }

    /// Equivalent to `Vec2::from_angle(self.to_radians_f32())`.
    pub const fn to_vec2(self) -> Vec2 {
        match self {
            Self::Zero => Vec2::X,
            Self::Ninety => Vec2::Y,
            Self::OneEighty => Vec2::NEG_X,
            Self::TwoSeventy => Vec2::NEG_Y,
        }
    }

    /// Equivalent to `DVec2::from_angle(self.to_radians_f64())`.
    pub const fn to_dvec2(self) -> DVec2 {
        match self {
            Self::Zero => DVec2::X,
            Self::Ninety => DVec2::Y,
            Self::OneEighty => DVec2::NEG_X,
            Self::TwoSeventy => DVec2::NEG_Y,
        }
    }
}

impl std::fmt::Display for GridAngle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        (*self as u16).fmt(f)
    }
}

const impl std::ops::Neg for GridAngle {
    type Output = Self;
    fn neg(self) -> Self::Output {
        match self {
            GridAngle::Zero => Self::Zero,
            GridAngle::Ninety => Self::TwoSeventy,
            GridAngle::OneEighty => Self::OneEighty,
            GridAngle::TwoSeventy => Self::Ninety,
        }
    }
}

const impl std::ops::Add for GridAngle {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (GridAngle::Zero, rhs) => rhs,
            (lhs, GridAngle::Zero) => lhs,
            (GridAngle::Ninety, GridAngle::Ninety) | (GridAngle::TwoSeventy, GridAngle::TwoSeventy) => Self::OneEighty,
            (GridAngle::Ninety, GridAngle::OneEighty) | (GridAngle::OneEighty, GridAngle::Ninety) => Self::TwoSeventy,
            (GridAngle::Ninety, GridAngle::TwoSeventy) | (GridAngle::OneEighty, GridAngle::OneEighty) | (GridAngle::TwoSeventy, GridAngle::Ninety) => Self::Zero,
            (GridAngle::OneEighty, GridAngle::TwoSeventy) | (GridAngle::TwoSeventy, GridAngle::OneEighty) => Self::Ninety,
        }
    }
}

const impl std::ops::AddAssign for GridAngle {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

const impl std::ops::Sub for GridAngle {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        self + (-rhs)
    }
}

const impl std::ops::SubAssign for GridAngle {
    fn sub_assign(&mut self, rhs: Self) {
        *self += -rhs;
    }
}

#[derive(Debug, Clone, Copy, enum_map::Enum, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
#[enumflags2::bitflags]
#[repr(u8)]
#[expect(clippy::use_self, reason = "bitflags macro generated")]
pub enum Axis {
    X, Y, Z,
}

impl Axis {
    pub const VALUES: [Self; 3] = [
        Self::X, Self::Y, Self::Z,
    ];

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

    pub const fn as_dvec3(self) -> DVec3 {
        match self {
            Self::X => DVec3::X,
            Self::Y => DVec3::Y,
            Self::Z => DVec3::Z,
        }
    }
}

impl std::fmt::Display for Axis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::X => "x",
            Self::Y => "y",
            Self::Z => "z",
        }.fmt(f)
    }
}

const impl std::cmp::PartialEq for Axis {
    fn eq(&self, other: &Self) -> bool {
        match self {
            Axis::X => matches!(other, Axis::X),
            Axis::Y => matches!(other, Axis::Y),
            Axis::Z => matches!(other, Axis::Z),
        }
    }
}
const impl std::cmp::Eq for Axis { }

impl std::hash::Hash for Axis {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
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

    pub const fn rotate_90_degrees_cw(self, axis: Axis) -> Self {
        #[expect(clippy::match_same_arms, reason = "more clear with each arm separate")]
        match (self, axis) {
            (Self::PosX, Axis::X) => Self::PosX,
            (Self::NegX, Axis::X) => Self::NegX,
            (Self::PosY, Axis::Y) => Self::PosY,
            (Self::NegY, Axis::Y) => Self::NegY,
            (Self::PosZ, Axis::Z) => Self::PosZ,
            (Self::NegZ, Axis::Z) => Self::NegZ,

            (Self::PosX, Axis::Y) => Self::NegZ,
            (Self::PosX, Axis::Z) => Self::PosY,
            (Self::NegX, Axis::Y) => Self::PosZ,
            (Self::NegX, Axis::Z) => Self::NegY,
            (Self::PosY, Axis::X) => Self::PosZ,
            (Self::PosY, Axis::Z) => Self::NegX,
            (Self::NegY, Axis::X) => Self::NegZ,
            (Self::NegY, Axis::Z) => Self::PosX,
            (Self::PosZ, Axis::X) => Self::NegY,
            (Self::PosZ, Axis::Y) => Self::PosX,
            (Self::NegZ, Axis::X) => Self::PosY,
            (Self::NegZ, Axis::Y) => Self::NegX,
        }
    }

    pub const fn rotate_90_degrees_ccw(self, axis: Axis) -> Self {
        self.rotate_90_degrees_cw(axis).rotate_90_degrees_cw(axis).rotate_90_degrees_cw(axis)
    }

    pub const fn rotate(self, axis: Axis, angle: GridAngle) -> Self {
        match (self, axis, angle) {
            (_, _, GridAngle::Zero) |
            (Self::PosX | Self::NegX, Axis::X, _) |
            (Self::PosY | Self::NegY, Axis::Y, _) |
            (Self::PosZ | Self::NegZ, Axis::Z, _) => self,
            (_, _, GridAngle::OneEighty) => self.opposit(),
            (_, _, GridAngle::Ninety) => self.rotate_90_degrees_cw(axis),
            (_, _, GridAngle::TwoSeventy) => self.rotate_90_degrees_ccw(axis),
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

    pub const fn as_vec3(self) -> Vec3 {
        match self {
            Self::PosX => Vec3::X,
            Self::NegX => Vec3::NEG_X,
            Self::PosY => Vec3::Y,
            Self::NegY => Vec3::NEG_Y,
            Self::PosZ => Vec3::Z,
            Self::NegZ => Vec3::NEG_Z,
        }
    }

    pub const fn as_dvec3(self) -> DVec3 {
        match self {
            Self::PosX => DVec3::X,
            Self::NegX => DVec3::NEG_X,
            Self::PosY => DVec3::Y,
            Self::NegY => DVec3::NEG_Y,
            Self::PosZ => DVec3::Z,
            Self::NegZ => DVec3::NEG_Z,
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

impl std::fmt::Display for CardinalDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PosX => "+x",
            Self::NegX => "-x",
            Self::PosY => "+y",
            Self::NegY => "-y",
            Self::PosZ => "+z",
            Self::NegZ => "-z",
        }.fmt(f)
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

#[test]
fn test_grid_angle_add() {
    for a in GridAngle::VALUES {
        for b in GridAngle::VALUES {
            assert_eq!((a + b) as u16, (a as u16 + b as u16) % 360, "{a} + {b}");
        }
    }
}

#[test]
fn test_grid_angle_sub() {
    for a in GridAngle::VALUES {
        for b in GridAngle::VALUES {
            assert_eq!(i32::from((a - b) as u16), (i32::from(a as u16) - i32::from(b as u16)).rem_euclid(360), "{a} - {b}");
        }
    }
}

#[test]
fn test_cardinal_direction_rotate_90() {
    for dir in CardinalDirection::VALUES {
        for axis in Axis::VALUES {
            if dir.axis() == axis {
                assert_eq!(dir.rotate_90_degrees_cw(axis), dir, "{dir} arount {axis}");
            }
            else {
                assert_eq!(dir.rotate_90_degrees_cw(axis).rotate_90_degrees_cw(axis), dir.opposit(), "{dir} around {axis}");
            }

            assert_eq!(dir.rotate_90_degrees_cw(axis).rotate_90_degrees_ccw(axis), dir, "{dir} around {axis}");
            assert_eq!(dir.rotate_90_degrees_ccw(axis).rotate_90_degrees_cw(axis), dir, "{dir} around {axis}");
        }
    }
}

#[test]
fn test_cardinal_direction_rotation() {
    for angle in GridAngle::VALUES {
        for dir in CardinalDirection::VALUES {
            for axis in Axis::VALUES {
                let expected = dir.as_dvec3().rotate_axis(axis.as_dvec3(), angle.to_radians_f64());
                let expected = (expected * 1e6f64).round() * 1e-6f64;
                assert_eq!(dir.rotate(axis, angle).as_dvec3(), expected, "{dir} around {axis} for {angle}");
            }
        }
    }
}

#[test]
fn test_grid_angle_to_vec2() {
    for angle in GridAngle::VALUES {
        let expected = Vec2::from_angle(angle.to_radians_f32());
        let expected = (expected * 1e6f32).round() * 1e-6f32;
        assert_eq!(angle.to_vec2(), expected, "{angle}");
    }
}
