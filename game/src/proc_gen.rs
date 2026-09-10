#![allow(clippy::default_numeric_fallback, reason = "math")]

use glam::{DVec2, DVec3, ISizeVec3, Vec3Swizzles as _};

use noise::{HybridMulti, NoiseFn as _, Simplex};
use rand::{Rng as _, RngExt as _, SeedableRng as _, rngs::SmallRng};
use crate::{chunk::{BlockData, CHUNK_SIZE, Chunk}, resource_location::location, utils::Vec3Range};

const MAX_HEIGHT: f64 = 16.;
const MIN_HEIGHT: f64 = -16.;

pub struct Generator {
    #[expect(dead_code, reason = "not yet used")]
    rng: SmallRng,
    backbone_offset: DVec3,
    backbone_noise: HybridMulti<Simplex>,
    heightmap_offset: DVec2,
    heightmap_noise: HybridMulti<Simplex>,
}

impl Generator {
    pub fn new(seed: u64) -> Self {
        let mut rng = SmallRng::seed_from_u64(seed);
        Self {
            backbone_offset: rng.random(),
            backbone_noise: HybridMulti::new(rng.next_u32())
                .set_sources(std::iter::repeat_with(|| Simplex::new(rng.next_u32())).take(32).collect()),
            heightmap_offset: rng.random(),
            heightmap_noise: HybridMulti::new(rng.next_u32())
                .set_sources(std::iter::repeat_with(|| Simplex::new(rng.next_u32())).take(32).collect()),
            rng,
        }
    }

    fn height_at(&self, pos: DVec2) -> f64 {
        let pos = pos * 0.003 + self.heightmap_offset;
        let height = self.heightmap_noise.get(pos.to_array());
        f64::mul_add(f64::midpoint(height, 1.), MAX_HEIGHT - MIN_HEIGHT, MIN_HEIGHT)
    }

    fn backbone_at(&self, pos: DVec3) -> f64 {
        let pos = pos * 0.015 + self.backbone_offset;
        self.backbone_noise.get(pos.to_array()) / 4.
    }

    fn generate_block(&self, pos: ISizeVec3) -> bool {
        let pos = pos.as_dvec3();
        let height = self.height_at(pos.xz());
        let diff = pos.y - height;
        let final_value = (diff / 16.) + self.backbone_at(pos);

        final_value < 0.
    }

    pub fn generate_chunk(&self, chunk_pos: ISizeVec3) -> Chunk {
        let mut chunk = Chunk::new();
        let filled_block = BlockData {
            id: location!("minecraft:dirt"),
            state: String::new(),
        };

        for delta_pos in Vec3Range(glam::USizeVec3::ZERO, CHUNK_SIZE) {
            let global_pos = chunk_pos * CHUNK_SIZE.as_isizevec3() + delta_pos.as_isizevec3();
            if self.generate_block(global_pos) {
                chunk.set(delta_pos, &filled_block);
            }
        }

        chunk
    }
}
