#![allow(clippy::default_numeric_fallback, reason = "math")]
#![allow(clippy::cast_precision_loss, reason = "math")]

use glam::{DVec2, DVec3, ISizeVec3, USizeVec2, USizeVec3, Vec3Swizzles as _};

use noise::{HybridMulti, NoiseFn as _, Simplex};
use rand::{Rng as _, RngExt as _, SeedableRng as _, rngs::SmallRng};
use crate::{chunk::{BlockData, CHUNK_SIZE, Chunk}, resource_location::location};

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

    fn generate_block(&self, pos: DVec3, height: f64) -> bool {
        let diff = pos.y - height;
        let final_value = (diff / 16.) + self.backbone_at(pos);

        final_value < 0.
    }

    pub fn generate_chunk(&self, chunk_pos: ISizeVec3) -> Chunk {
        let mut chunk = Chunk::new();
        let top_block = BlockData { id: location!("minecraft:stone"), state: String::new(), };
        let bottom_block = BlockData { id: location!("minecraft:stone"), state: String::new(), };

        let chunk_offset = chunk_pos * CHUNK_SIZE.as_isizevec3();
        for dx in 0..CHUNK_SIZE.x {
            for dz in 0..CHUNK_SIZE.z {
                let delta_pos2d = USizeVec2::new(dx, dz);
                let global_pos2d = chunk_offset.xz() + delta_pos2d.as_isizevec2();
                let global_pos2d = global_pos2d.as_dvec2();
                let height = self.height_at(global_pos2d);

                let mut prev = self.generate_block(DVec3::new(global_pos2d.x, chunk_offset.y as f64 + CHUNK_SIZE.y as f64, global_pos2d.y), height);
                for dy in (0..CHUNK_SIZE.y).rev() {
                    let delta_pos = USizeVec3::new(dx, dy, dz);
                    let global_y = chunk_offset.y.wrapping_add_unsigned(dy);
                    let global_pos = DVec3::new(global_pos2d.x, global_y as f64, global_pos2d.y);
                    if self.generate_block(global_pos, height) {
                        if prev {
                            chunk.set(delta_pos, &bottom_block);
                        }
                        else {
                            chunk.set(delta_pos, &top_block);
                        }
                        prev = true;
                    }
                    else {
                        prev = false;
                    }
                }
            }
        }

        chunk
    }
}
