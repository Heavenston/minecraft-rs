use glam::Vec2Swizzles;

use super::{ Axis, CardinalDirection };

pub const trait AxisVec2Ext: Vec2Swizzles {
    type Value;
    fn with_axis(self, axis: Axis, value: Self::Value) -> Self::Vec3;
}

pub const trait DirectionVec2Ext: Vec2Swizzles {
    type Value;
    fn with_direction(self, direction: CardinalDirection) -> Self::Vec3;
}

macro_rules! impl_for_glam_vecs {
    () => { };
    ($name:ty, $ty2d:ty, $ty:ty$(, $signed:tt)?;$($rest:tt)*) => {
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
            type Output = $ty2d;

            fn sub(self, rhs: Axis) -> Self::Output {
                match rhs {
                    Axis::X => <$ty2d>::new(self.y, self.z),
                    Axis::Y => <$ty2d>::new(self.x, self.z),
                    Axis::Z => <$ty2d>::new(self.x, self.y),
                }
            }
        }

        const impl AxisVec2Ext for $ty2d {
            type Value = $ty;
            fn with_axis(self, axis: Axis, value: $ty) -> $name {
                match axis {
                    Axis::X => <$name>::new(value, self.x, self.y),
                    Axis::Y => <$name>::new(self.x, value, self.y),
                    Axis::Z => <$name>::new(self.x, self.y, value),
                }
            }
        }

        impl_for_glam_vecs!(@ $name, $ty2d, $ty$(, $signed)?);
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
