#![expect(dead_code, reason = "Utils with functions maybe not used")]
#![expect(unused_imports, reason = "Utils with functions maybe not used")]

use glam::{DVec2, DVec3, ISizeVec3, USizeVec3, Vec2, Vec3};

pub mod enum_set;
pub use enum_set::{ EnumSet };
mod glam_ext;
pub use glam_ext::*;

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

    pub const fn into_bits(self) -> u16 {
        match self {
            Self::Zero => 0,
            Self::Ninety => 1,
            Self::OneEighty => 2,
            Self::TwoSeventy => 3,
        }
    }

    pub const fn from_bits(value: u16) -> Self {
        match value {
            0 => Self::Zero,
            1 => Self::Ninety,
            2 => Self::OneEighty,
            _ => Self::TwoSeventy,
        }
    }

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, enum_map::Enum, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Axis {
    X, Y, Z,
}

impl Axis {
    pub const VALUES: [Self; 3] = [
        Self::X, Self::Y, Self::Z,
    ];

    pub const fn pos(self) -> CardinalDirection {
        CardinalDirection { sign: Sign::Positive, axis: self }
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

    pub const fn as_isizevec3(self) -> ISizeVec3 {
        match self {
            Self::X => ISizeVec3::X,
            Self::Y => ISizeVec3::Y,
            Self::Z => ISizeVec3::Z,
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

impl enum_set::Enum for Axis {
    type Integer = u8;
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

#[derive(serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, enum_map::Enum)]
pub enum Sign {
    Positive,
    Negative,
}

impl Sign {
    pub const VALUES: [Self; 2] = [Self::Positive, Self::Negative];

    pub const fn as_i8(self) -> i8 {
        match self {
            Self::Positive =>  1,
            Self::Negative => -1,
        }
    }

    pub const fn as_isize(self) -> isize {
        self.as_i8().into()
    }

    pub const fn as_f32(self) -> f32 {
        self.as_i8().into()
    }

    pub const fn as_f64(self) -> f64 {
        self.as_i8().into()
    }

    pub const fn is_positive(self) -> bool {
        matches!(self, Self::Positive)
    }

    pub const fn is_negative(self) -> bool {
        matches!(self, Self::Negative)
    }
}

const impl std::ops::Neg for Sign {
    type Output = Self;
    fn neg(self) -> Self::Output {
        match self {
            Self::Positive => Self::Negative,
            Self::Negative => Self::Positive,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, enum_map::Enum)]
pub struct CardinalDirection {
    pub sign: Sign,
    pub axis: Axis,
}

#[expect(non_upper_case_globals, reason = "This replaces the existing usage for when this was an enum")]
impl CardinalDirection {
    pub const VALUES: [Self; 6] = std::array::from_fn::<_, { Sign::VALUES.len() * Axis::VALUES.len() }, _>(const |i| {
        Self {
            sign: Sign::VALUES[i % Sign::VALUES.len()],
            axis: Axis::VALUES[i / Sign::VALUES.len()],
        }
    });

    #[doc(alias = "East")]
    pub const PosX: Self = Self { sign: Sign::Positive, axis: Axis::X };
    #[doc(alias = "West")]
    pub const NegX: Self = Self { sign: Sign::Negative, axis: Axis::X };
    #[doc(alias = "Up")]
    pub const PosY: Self = Self { sign: Sign::Positive, axis: Axis::Y };
    #[doc(alias = "Down")]
    pub const NegY: Self = Self { sign: Sign::Negative, axis: Axis::Y };
    #[doc(alias = "South")]
    pub const PosZ: Self = Self { sign: Sign::Positive, axis: Axis::Z };
    #[doc(alias = "North")]
    pub const NegZ: Self = Self { sign: Sign::Negative, axis: Axis::Z };

    pub const fn opposit(self) -> Self {
        Self {
            sign: -self.sign,
            axis: self.axis,
        }
    }

    pub const fn rotate_90_degrees_cw(self, rotate_axis: Axis) -> Self {
        #[expect(clippy::match_same_arms, reason = "more clear with each arm separate")]
        match (self, rotate_axis) {
            (Self { axis: Axis::X, .. }, Axis::X) |
            (Self { axis: Axis::Y, .. }, Axis::Y) |
            (Self { axis: Axis::Z, .. }, Axis::Z) => self,

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
        let ISizeVec3 { x, y, z } = self.axis.as_isizevec3();
        let sign = self.sign.as_isize();
        ISizeVec3::new(x * sign, y * sign, z * sign)
    }

    pub const fn as_vec3(self) -> Vec3 {
        let Vec3 { x, y, z } = self.axis.as_vec3();
        let sign = self.sign.as_f32();
        Vec3::new(x * sign, y * sign, z * sign)
    }

    pub const fn as_dvec3(self) -> DVec3 {
        let DVec3 { x, y, z } = self.axis.as_dvec3();
        let sign = self.sign.as_f64();
        DVec3::new(x * sign, y * sign, z * sign)
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
        match *self {
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

impl enum_set::Enum for CardinalDirection {
    type Integer = u8;
}

impl<'de> serde::Deserialize<'de> for CardinalDirection {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: serde::Deserializer<'de> {
        struct Visitor;
        impl serde::de::Visitor<'_> for Visitor {
            type Value = CardinalDirection;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(formatter, "a cardinal direction")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where E: serde::de::Error,
            {
                match v {
                    "east"  => Ok(CardinalDirection::PosX),
                    "west"  => Ok(CardinalDirection::NegX),
                    "up"    => Ok(CardinalDirection::PosY),
                    "down"  => Ok(CardinalDirection::NegY),
                    "south" => Ok(CardinalDirection::PosZ),
                    "north" => Ok(CardinalDirection::NegZ),
                    _ => Err(E::invalid_value(serde::de::Unexpected::Str(v), &"one of: 'east', 'west', 'up', 'down', 'south' or 'north'"))
                }
            }
        }
        deserializer.deserialize_str(Visitor)
    }
}

#[test]
fn test_cardinal_direction_indices() {
    assert_eq!(enum_map::Enum::into_usize(CardinalDirection::PosX), 0);
    assert_eq!(enum_map::Enum::into_usize(CardinalDirection::NegX), 1);
    assert_eq!(enum_map::Enum::into_usize(CardinalDirection::PosY), 2);
    assert_eq!(enum_map::Enum::into_usize(CardinalDirection::NegY), 3);
    assert_eq!(enum_map::Enum::into_usize(CardinalDirection::PosZ), 4);
    assert_eq!(enum_map::Enum::into_usize(CardinalDirection::NegZ), 5);
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
            if dir.axis == axis {
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
