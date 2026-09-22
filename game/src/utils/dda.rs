use glam::{BVec3, ISizeVec3, Vec3};

use crate::utils::CardinalDirection;

pub struct Hit {
    pub voxel: ISizeVec3,
    pub t: f32,
    pub normal: ISizeVec3,
}

pub struct Dda {
    voxel: ISizeVec3,
    step: ISizeVec3,
    t_max: Vec3,
    t_delta: Vec3,
    t: f32,
    mask: BVec3,
}

impl Dda {
    pub fn new(origin: Vec3, dir: Vec3) -> Self {
        let dir = dir.normalize();
        let voxel = origin.floor().as_isizevec3();
        let step = dir.signum().as_isizevec3();
        let t_delta = dir.recip().abs();
        let frac = origin - voxel.as_vec3();
        let to_edge = Vec3::select(dir.cmpgt(Vec3::ZERO), 1.0 - frac, frac);
        let t_max = Vec3::select(dir.cmpeq(Vec3::ZERO), Vec3::INFINITY, to_edge * t_delta);

        Self { voxel, step, t_max, t_delta, t: 0.0, mask: BVec3::FALSE }
    }
}

impl Iterator for Dda {
    type Item = Hit;

    fn next(&mut self) -> Option<Hit> {
        let hit = Hit {
            voxel: self.voxel,
            t: self.t,
            normal: ISizeVec3::select(self.mask, -self.step, ISizeVec3::ZERO),
        };

        self.t = self.t_max.min_element();
        self.mask = self.t_max.cmpeq(Vec3::splat(self.t));
        self.voxel += ISizeVec3::select(self.mask, self.step, ISizeVec3::ZERO);
        self.t_max += Vec3::select(self.mask, self.t_delta, Vec3::ZERO);

        Some(hit)
    }
}
