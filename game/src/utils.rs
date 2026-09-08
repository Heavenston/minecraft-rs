use glam::I8Vec3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
