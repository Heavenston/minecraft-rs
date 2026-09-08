use glam::I8Vec3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, enum_map::Enum)]
pub enum Axis {
    X, Y, Z,
}

impl Axis {
    pub fn pos(self) -> CardinalDirection {
        match self {
            Self::X => CardinalDirection::PosX,
            Self::Y => CardinalDirection::PosY,
            Self::Z => CardinalDirection::PosZ,
        }
    }

    pub fn neg(self) -> CardinalDirection {
        -self.pos()
    }

    pub fn as_vector(self) -> I8Vec3 {
        self.pos().as_vector()
    }
}

macro_rules! impl_index_axis {
    () => {};
    ($name:ty, $ty:ty$(;$($rest:tt)*)?) => {
        impl std::ops::Index<Axis> for $name {
            type Output = $ty;

            fn index(&self, index: Axis) -> &Self::Output {
                match index {
                    Axis::X => &self.x,
                    Axis::Y => &self.y,
                    Axis::Z => &self.z,
                }
            }
        }

        impl std::ops::IndexMut<Axis> for $name {
            fn index_mut(&mut self, index: Axis) -> &mut Self::Output {
                match index {
                    Axis::X => &mut self.x,
                    Axis::Y => &mut self.y,
                    Axis::Z => &mut self.z,
                }
            }
        }
        impl_index_axis!($($($rest)*)?);
    };
}
impl_index_axis!(
    glam::I8Vec3, i8 ; glam::I16Vec3, i16; glam::IVec3, i32 ; glam::I64Vec3, i64;
    glam::U8Vec3, u8 ; glam::U16Vec3, u16; glam::UVec3, u32 ; glam::U64Vec3, u64;
    glam::Vec3  , f32; glam::DVec3  , f64; glam::BVec3, bool;
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, enum_map::Enum)]
pub enum CardinalDirection {
    PosX,
    NegX,
    PosY,
    NegY,
    PosZ,
    NegZ,
}

impl CardinalDirection {
    pub fn opposit(self) -> Self {
        match self {
            Self::PosX => Self::NegX,
            Self::NegX => Self::PosX,
            Self::PosY => Self::NegY,
            Self::NegY => Self::PosY,
            Self::PosZ => Self::NegZ,
            Self::NegZ => Self::PosZ,
        }
    }

    pub fn as_vector(self) -> I8Vec3 {
        match self {
            Self::PosX => I8Vec3::X,
            Self::NegX => I8Vec3::NEG_X,
            Self::PosY => I8Vec3::Y,
            Self::NegY => I8Vec3::NEG_Y,
            Self::PosZ => I8Vec3::Z,
            Self::NegZ => I8Vec3::NEG_Z,
        }
    }

    pub fn axis(self) -> Axis {
        match self {
            Self::PosX | Self::NegX => Axis::X,
            Self::PosY | Self::NegY => Axis::Y,
            Self::PosZ | Self::NegZ => Axis::Z,
        }
    }

    pub fn abs(self) -> Self {
        Self::from(self.axis())
    }

    pub fn is_positive(self) -> bool { self.abs() == self }
    pub fn is_negative(self) -> bool { !self.is_positive() }
}

impl From<Axis> for CardinalDirection {
    fn from(val: Axis) -> Self {
        match val {
            Axis::X => Self::PosX,
            Axis::Y => Self::PosY,
            Axis::Z => Self::PosZ,
        }
    }
}

impl std::ops::Neg for CardinalDirection {
    type Output = Self;

    fn neg(self) -> Self::Output {
        self.opposit()
    }
}

impl std::str::FromStr for CardinalDirection {
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
