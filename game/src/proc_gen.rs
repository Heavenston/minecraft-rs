#![allow(clippy::default_numeric_fallback, reason = "math")]

use glam::{DVec2, DVec3, ISizeVec2, ISizeVec3, Vec3Swizzles as _};

use noise::{NoiseFn as _, Perlin};
use rand::{Rng as _, RngExt as _, SeedableRng as _, rngs::SmallRng};
use crate::{chunk::{BlockData, CHUNK_SIZE, Chunk}, data_extractor::blockstate::BlockState, resource_location::{ResourceLocation, location}, utils::Vec3Range};

const MAX_HEIGHT: f64 = 32.;
const MIN_HEIGHT: f64 = -32.;

pub struct Generator {
    rng: SmallRng,
    backbone_offset: DVec3,
    backbone_noise: Perlin,
    heightmap_offset: DVec2,
    heightmap_noise: Perlin,
}

impl Generator {
    pub fn new(seed: u64) -> Self {
        let mut rng = SmallRng::seed_from_u64(seed);
        Self {
            backbone_offset: DVec3::new(rng.random_range(-1f64..=1f64), rng.random_range(-1f64..=1f64), rng.random_range(-1f64..=1f64)),
            backbone_noise: Perlin::new(rng.next_u32()),
            heightmap_offset: DVec2::new(rng.random_range(-1f64..=1f64), rng.random_range(-1f64..=1f64)),
            heightmap_noise: Perlin::new(rng.next_u32()),
            rng,
        }
    }

    fn height_at(&self, pos: DVec2) -> f64 {
        let pos = pos * 10e-4 + self.heightmap_offset;
        let height = self.heightmap_noise.get(pos.to_array());
        f64::mul_add(f64::midpoint(height, 1.), MAX_HEIGHT, MIN_HEIGHT)
    }

    fn backbone_at(&self, pos: DVec3) -> f64 {
        let pos = pos * 10e-3 + self.backbone_offset;
        self.backbone_noise.get(pos.to_array()) / 2.
    }

    fn generate_block(&self, pos: ISizeVec3) -> bool {
        const NOISE_REACH_LOW: f64 = -16.;
        const NOISE_REACH_HIGH: f64 = 16.;

        let pos = pos.as_dvec3();
        let height = self.height_at(pos.xz());
        let diff = height - pos.y;
        let final_value = (diff / 16.) + self.backbone_at(pos);

        final_value < 0.
    }

    pub fn generate_chunk(&self, chunk_pos: ISizeVec3) -> Chunk {
        let mut chunk = Chunk::new();
        let filled_block = BlockData {
            id: location!("minecraft:stone"),
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
