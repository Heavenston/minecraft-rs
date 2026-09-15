#![expect(clippy::default_numeric_fallback, reason = "Macro generated")]

use glam::{DMat4, DVec2, DVec3, ISizeVec2, ISizeVec3, Mat4, USizeVec2, USizeVec3, Vec2, Vec3, Vec3Swizzles};
use crate::utils::{ glam_ext::{ ConstVec as _, ConstBVec as _, AxisVec2Ext }, CardinalDirection, Sign };

macro_rules! impl_aabb {
    (@decl float, $aabb: ident, $vec: ident) => {
        #[derive(Default, Debug, Clone, Copy, PartialEq)]
        pub struct $aabb {
            pub min: $vec,
            pub max: $vec,
        }
    };
    (@decl int, $aabb: ident, $vec: ident) => {
        #[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $aabb {
            pub min: $vec,
            pub max: $vec,
        }
    };

    (@vertices 2, $scalar: ty, $signed: tt $float: tt, $aabb: ident, $vec: ident, $aabb3d: ident,) => {
        impl $aabb {
            /// Returns all vertices of this aabb in counter clockwise order starting
            /// with the vertex with minimum axis values.
            pub const fn vertices(self) -> [$vec; 4] {
                [
                    self.min,
                    $vec::new(self.max.x, self.min.y),
                    self.max,
                    $vec::new(self.min.x, self.max.y),
                ]
            }

            /// Implementation of face_aabb for 3d aabbs, but put here to have
            /// access to $vec in the macro
            const fn face_aabb(aabb: $aabb3d, face: CardinalDirection) -> $aabb {
                match face {
                    CardinalDirection::PosX => $aabb {
                        min: $vec::new(aabb.min.y,aabb.max.z),
                        max: $vec::new(aabb.max.y,aabb.max.z),
                    },
                    CardinalDirection::NegX => $aabb {
                        min: $vec::new(aabb.min.y,aabb.min.z),
                        max: $vec::new(aabb.max.y,aabb.min.z),
                    },
                    CardinalDirection::PosY => $aabb {
                        min: $vec::new(aabb.min.x,aabb.max.z),
                        max: $vec::new(aabb.min.x,aabb.min.z),
                    },
                    CardinalDirection::NegY => $aabb {
                        min: $vec::new(aabb.min.x,aabb.min.z),
                        max: $vec::new(aabb.min.x,aabb.max.z),
                    },
                    CardinalDirection::PosZ => $aabb {
                        min: $vec::new(aabb.min.x,aabb.min.y),
                        max: $vec::new(aabb.min.x,aabb.max.y),
                    },
                    CardinalDirection::NegZ => $aabb {
                        min: $vec::new(aabb.max.x,aabb.min.y),
                        max: $vec::new(aabb.max.x,aabb.max.y),
                    },
                }
            }
        }
    };
    (@vertices 3, $scalar: ty, $signed: tt $float: tt, $aabb: ident, $vec: ident, $aabb2d: ident,) => {
        impl $aabb {
            pub const fn face_aabb(self, face: CardinalDirection) -> $aabb2d {
                $aabb2d::face_aabb(self, face)
            }

            pub const fn direction_bound(self, dir: CardinalDirection) -> $scalar {
                (match dir.sign {
                    Sign::Negative => self.min,
                    Sign::Positive => self.min,
                })[dir.axis]
            }

            /// Returns the vertices of the face facing the given direction.
            /// The vertices are returned in counter clockwise order starting with the
            /// bottom left one.
            pub const fn face_vertices(self, face: CardinalDirection) -> [$vec; 4] {
                match face {
                    CardinalDirection::PosX => [
                        $vec::new(self.max.x,self.min.y,self.max.z),
                        $vec::new(self.max.x,self.min.y,self.min.z),
                        $vec::new(self.max.x,self.max.y,self.min.z),
                        $vec::new(self.max.x,self.max.y,self.max.z),
                    ],
                    CardinalDirection::NegX => [
                        $vec::new(self.min.x,self.min.y,self.min.z),
                        $vec::new(self.min.x,self.min.y,self.max.z),
                        $vec::new(self.min.x,self.max.y,self.max.z),
                        $vec::new(self.min.x,self.max.y,self.min.z),
                    ],
                    CardinalDirection::PosY => [
                        $vec::new(self.min.x,self.max.y,self.max.z),
                        $vec::new(self.max.x,self.max.y,self.max.z),
                        $vec::new(self.max.x,self.max.y,self.min.z),
                        $vec::new(self.min.x,self.max.y,self.min.z),
                    ],
                    CardinalDirection::NegY => [
                        $vec::new(self.min.x,self.min.y,self.min.z),
                        $vec::new(self.max.x,self.min.y,self.min.z),
                        $vec::new(self.max.x,self.min.y,self.max.z),
                        $vec::new(self.min.x,self.min.y,self.max.z),
                    ],
                    CardinalDirection::PosZ => [
                        $vec::new(self.min.x,self.min.y,self.max.z),
                        $vec::new(self.max.x,self.min.y,self.max.z),
                        $vec::new(self.max.x,self.max.y,self.max.z),
                        $vec::new(self.min.x,self.max.y,self.max.z),
                    ],
                    CardinalDirection::NegZ => [
                        $vec::new(self.max.x,self.min.y,self.min.z),
                        $vec::new(self.min.x,self.min.y,self.min.z),
                        $vec::new(self.min.x,self.max.y,self.min.z),
                        $vec::new(self.max.x,self.max.y,self.min.z),
                    ],
                }
            }
        }
    };

    (@frustrum 3, signed float, $aabb: ident, $vec: ident, $mat: ident,) => {
        impl $aabb {
            /// Returns true if the furstrum defined by the given view projection
            /// intersects the aabb.
            pub fn frustrum_test(self, view_projection: &$mat) -> bool {
                type Vec4 = <$vec as Vec3Swizzles>::Vec4;
                // Use our min max to define eight corners
                let corners: [Vec4; 8] = [
                    Vec4::new(self.min.x, self.min.y, self.min.z, 1.0), // x y z
                    Vec4::new(self.max.x, self.min.y, self.min.z, 1.0), // X y z
                    Vec4::new(self.min.x, self.max.y, self.min.z, 1.0), // x Y z
                    Vec4::new(self.max.x, self.max.y, self.min.z, 1.0), // X Y z

                    Vec4::new(self.min.x, self.min.y, self.max.z, 1.0), // x y Z
                    Vec4::new(self.max.x, self.min.y, self.max.z, 1.0), // X y Z
                    Vec4::new(self.min.x, self.max.y, self.max.z, 1.0), // x Y Z
                    Vec4::new(self.max.x, self.max.y, self.max.z, 1.0), // X Y Z
                ];
                let corners = corners.map(|corner| view_projection * corner);

                !(
                    // left and right
                    (corners.iter().all(|corner| corner.x < -corner.w) || corners.iter().all(|corner| corner.x > corner.w)) &&
                    // bottom and top
                    (corners.iter().all(|corner| corner.y < -corner.w) || corners.iter().all(|corner| corner.y > corner.w)) &&
                    // near and far
                    (corners.iter().all(|corner| corner.z < 0.) || corners.iter().all(|corner| corner.z > corner.w))
                )
            }
        }
    };
    (@frustrum $dim: tt, $signed: tt $float: tt, $aabb: ident, $vec: ident,) => {
    };

    ($dim: tt, $scalar: ty, $signed: tt $float: tt, $aabb: ident, $vec: ident,$(aabb2d $aabb2d: ident,)?$(aabb3d $aabb3d: ident,)?$(mat $mat: ident,)?) => {
        impl_aabb!(@decl $float, $aabb, $vec);

        impl_aabb!(@vertices $dim, $scalar, $signed $float, $aabb, $vec, $($aabb2d,)? $($aabb3d,)?);
        impl_aabb!(@frustrum $dim, $signed $float, $aabb, $vec, $($mat,)?);

        impl $aabb {
            pub const fn is_empty(self) -> bool {
                (self.min.const_cmpge(self.max)).const_any()
            }

            pub const fn intersection(self, other: Self) -> Self {
                Self {
                    min: self.min.const_max(other.min),
                    max: self.max.const_min(other.max),
                }
            }

            pub const fn union(self, other: Self) -> Self {
                Self {
                    min: self.min.const_min(other.min),
                    max: self.max.const_max(other.max),
                }
            }

            pub const fn extend_to_contain(self, other: $vec) -> Self {
                Self {
                    min: self.min.const_min(other),
                    max: self.max.const_max(other),
                }
            }

            pub const fn intersects(self, other: Self) -> bool {
                !(self & other).is_empty()
            }
        }

        const impl std::ops::BitAnd<$aabb> for $aabb  {
            type Output = $aabb;
            fn bitand(self, rhs: Self) -> Self {
                self.intersection(rhs)
            }
        }

        const impl std::ops::BitOr<$aabb> for $aabb  {
            type Output = $aabb;
            fn bitor(self, rhs: Self) -> Self {
                self.union(rhs)
            }
        }

        const impl std::ops::BitAnd<$vec> for $aabb  {
            type Output = $aabb;
            fn bitand(self, rhs: $vec) -> Self {
                self.extend_to_contain(rhs)
            }
        }
    };
}
impl_aabb!(2, f32  , signed   float, AABB2     , Vec2     , aabb3d AABB3     ,);
impl_aabb!(2, f64  , signed   float, DAABB2    , DVec2    , aabb3d DAABB3    ,);
impl_aabb!(2, usize, unsigned int  , USizeAABB2, USizeVec2, aabb3d USizeAABB3,);
impl_aabb!(2, isize, signed   int  , ISizeAABB2, ISizeVec2, aabb3d ISizeAABB3,);
impl_aabb!(3, f32  , signed   float, AABB3     , Vec3     , aabb2d AABB2     , mat Mat4 ,);
impl_aabb!(3, f64  , signed   float, DAABB3    , DVec3    , aabb2d DAABB2    , mat DMat4,);
impl_aabb!(3, usize, unsigned int  , USizeAABB3, USizeVec3, aabb2d USizeAABB2,);
impl_aabb!(3, isize, signed   int  , ISizeAABB3, ISizeVec3, aabb2d ISizeAABB2,);
