use glam::{ BVec2, BVec3, Vec2Swizzles, Vec3Swizzles };

use super::{ Axis, CardinalDirection };

pub const trait ConstBVec {
    fn const_all(self) -> bool;
    fn const_any(self) -> bool;
}

const impl ConstBVec for BVec2 {
    fn const_all(self) -> bool {
        self.x && self.y
    }

    fn const_any(self) -> bool {
        self.x || self.y
    }
}

const impl ConstBVec for BVec3 {
    fn const_all(self) -> bool {
        self.x && self.y && self.z
    }

    fn const_any(self) -> bool {
        self.x || self.y || self.z
    }
}

pub const trait ConstVec {
    type BVec;
    /// Const version of glam .max methods on vectors.
    fn const_max(self, other: Self) -> Self;
    /// Const version of glam .min methods on vectors.
    fn const_min(self, other: Self) -> Self;

    fn const_cmpgt(self, other: Self) -> Self::BVec;
    fn const_cmpge(self, other: Self) -> Self::BVec;
    fn const_cmplt(self, other: Self) -> Self::BVec;
    fn const_cmple(self, other: Self) -> Self::BVec;
    fn const_cmpeq(self, other: Self) -> Self::BVec;
    fn const_cmpne(self, other: Self) -> Self::BVec;
}

pub const trait AxisVec2Ext: Vec2Swizzles {
    type Value;
    fn with_axis(self, axis: Axis, value: Self::Value) -> Self::Vec3;
}

pub const trait AxisVec3Ext: Vec3Swizzles {
    type Value;
    fn project(self, axis: Axis) -> Self::Vec2;
}

pub const trait DirectionVec2Ext: Vec2Swizzles {
    type Value;
    fn with_direction(self, direction: CardinalDirection) -> Self::Vec3;
}

macro_rules! impl_for_glam_vecs {
    () => { };
    ($vec3:ty, $vec2:ty, $scalar:ty$(, $signed:tt)?;$($rest:tt)*) => {
        const impl std::ops::Index<Axis> for $vec3 {
            type Output = $scalar;

            fn index(&self, index: Axis) -> &Self::Output {
                match index {
                    Axis::X => &self.x,
                    Axis::Y => &self.y,
                    Axis::Z => &self.z,
                }
            }
        }

        const impl std::ops::IndexMut<Axis> for $vec3 {
            fn index_mut(&mut self, index: Axis) -> &mut Self::Output {
                match index {
                    Axis::X => &mut self.x,
                    Axis::Y => &mut self.y,
                    Axis::Z => &mut self.z,
                }
            }
        }

        const impl std::ops::Sub<Axis> for $vec3 {
            type Output = $vec2;

            fn sub(self, rhs: Axis) -> Self::Output {
                match rhs {
                    Axis::X => <$vec2>::new(self.y, self.z),
                    Axis::Y => <$vec2>::new(self.x, self.z),
                    Axis::Z => <$vec2>::new(self.x, self.y),
                }
            }
        }

        const impl AxisVec2Ext for $vec2 {
            type Value = $scalar;
            fn with_axis(self, axis: Axis, value: $scalar) -> $vec3 {
                match axis {
                    Axis::X => <$vec3>::new(value, self.x, self.y),
                    Axis::Y => <$vec3>::new(self.x, value, self.y),
                    Axis::Z => <$vec3>::new(self.x, self.y, value),
                }
            }
        }

        const impl AxisVec3Ext for $vec3 {
            type Value = $scalar;
            fn project(self, axis: Axis) -> $vec2 {
                match axis {
                    Axis::X => <$vec2>::new(self.z, self.y),
                    Axis::Y => <$vec2>::new(self.x, self.z),
                    Axis::Z => <$vec2>::new(self.x, self.y),
                }
            }
        }

        const impl ConstVec for $vec3 {
            type BVec = BVec3;

            fn const_max(self, other: $vec3) -> $vec3 {
                Self { x: self.x.max(other.x), y: self.y.max(other.y), z: self.z.max(other.z) }
            }

            fn const_min(self, other: $vec3) -> $vec3 {
                Self { x: self.x.min(other.x), y: self.y.min(other.y), z: self.z.min(other.z) }
            }

            fn const_cmpgt(self, other: Self) -> Self::BVec {
                BVec3::new(self.x.gt(&other.x), self.y.gt(&other.y), self.z.gt(&other.z))
            }

            fn const_cmpge(self, other: Self) -> Self::BVec {
                BVec3::new(self.x.ge(&other.x), self.y.ge(&other.y), self.z.ge(&other.z))
            }

            fn const_cmplt(self, other: Self) -> Self::BVec {
                BVec3::new(self.x.lt(&other.x), self.y.lt(&other.y), self.z.lt(&other.z))
            }

            fn const_cmple(self, other: Self) -> Self::BVec {
                BVec3::new(self.x.le(&other.x), self.y.le(&other.y), self.z.le(&other.z))
            }

            fn const_cmpeq(self, other: Self) -> Self::BVec {
                BVec3::new(self.x.eq(&other.x), self.y.eq(&other.y), self.z.eq(&other.z))
            }

            fn const_cmpne(self, other: Self) -> Self::BVec {
                BVec3::new(self.x.ne(&other.x), self.y.ne(&other.y), self.z.ne(&other.z))
            }
        }

        const impl ConstVec for $vec2 {
            type BVec = BVec2;

            fn const_max(self, other: $vec2) -> $vec2 {
                Self { x: self.x.max(other.x), y: self.y.max(other.y) }
            }

            fn const_min(self, other: $vec2) -> $vec2 {
                Self { x: self.x.min(other.x), y: self.y.min(other.y) }
            }

            fn const_cmpgt(self, other: Self) -> Self::BVec {
                BVec2::new(self.x.gt(&other.x), self.y.gt(&other.y))
            }

            fn const_cmpge(self, other: Self) -> Self::BVec {
                BVec2::new(self.x.ge(&other.x), self.y.ge(&other.y))
            }

            fn const_cmplt(self, other: Self) -> Self::BVec {
                BVec2::new(self.x.lt(&other.x), self.y.lt(&other.y))
            }

            fn const_cmple(self, other: Self) -> Self::BVec {
                BVec2::new(self.x.le(&other.x), self.y.le(&other.y))
            }

            fn const_cmpeq(self, other: Self) -> Self::BVec {
                BVec2::new(self.x.eq(&other.x), self.y.eq(&other.y))
            }

            fn const_cmpne(self, other: Self) -> Self::BVec {
                BVec2::new(self.x.ne(&other.x), self.y.ne(&other.y))
            }
        }

        impl_for_glam_vecs!(@ $vec3, $vec2, $scalar$(, $signed)?);
        impl_for_glam_vecs!($($rest)*);
    };
    (@ $name:ty,$ty2d:ty,$ty:ty,signed) => {
        const impl DirectionVec2Ext for $ty2d {
            type Value = $ty;
            fn with_direction(self, direction: CardinalDirection) -> $name {
                self.with_axis(direction.axis, direction.sign.as_i8().into())
            }
        }
    };
    (@ $name:ty,$ty2d:ty,$ty:ty) => { };
}
impl_for_glam_vecs!(
    glam::I8Vec3   , glam::I8Vec2   , i8   , signed; glam::I16Vec3  , glam::I16Vec2  , i16  , signed; glam::IVec3, glam::IVec2, i32 , signed; glam::I64Vec3, glam::I64Vec2, i64, signed;
    glam::U8Vec3   , glam::U8Vec2   , u8           ; glam::U16Vec3  , glam::U16Vec2  , u16          ; glam::UVec3, glam::UVec2, u32         ; glam::U64Vec3, glam::U64Vec2, u64;
    glam::Vec3     , glam::Vec2     , f32  , signed; glam::DVec3    , glam::DVec2    , f64  , signed;
    glam::USizeVec3, glam::USizeVec2, usize        ; glam::ISizeVec3, glam::ISizeVec2, isize, signed;
);
