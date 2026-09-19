#![allow(clippy::default_numeric_fallback, reason = "math")]
#![allow(clippy::cast_precision_loss, reason = "math")]

use glam::{DVec2, DVec3, ISizeVec3, USizeVec2, USizeVec3, Vec3Swizzles as _};

use noise::{Fbm, MultiFractal as _, NoiseFn as _, Simplex};
use rand::{Rng as _, RngExt as _, SeedableRng as _, rngs::SmallRng};
use crate::{chunk::{BlockData, CHUNK_SIZE, Chunk, ChunkBlockIndex}, resource_location::location};

const MAX_HEIGHT: f64 = 64.;
const MIN_HEIGHT: f64 = -64.;

pub struct Generator {
    #[expect(dead_code, reason = "not yet used")]
    rng: SmallRng,
    backbone_offset: DVec3,
    backbone_noise: Fbm<Simplex>,
    heightmap_offset: DVec2,
    heightmap_noise: Fbm<Simplex>,
}

impl Generator {
    pub fn new(seed: u64) -> Self {
        let mut rng = SmallRng::seed_from_u64(seed);
        Self {
            backbone_offset: rng.random(),
            backbone_noise: Fbm::new(rng.next_u32())
                .set_octaves(8),
            heightmap_offset: rng.random(),
            heightmap_noise: Fbm::new(rng.next_u32())
                .set_octaves(6),
            rng,
        }
    }

    fn height_at(&self, pos: DVec2) -> f64 {
        let pos = pos * 0.001 + self.heightmap_offset;
        let height = self.heightmap_noise.get(pos.to_array());
        f64::mul_add(f64::midpoint(height, 1.), MAX_HEIGHT - MIN_HEIGHT, MIN_HEIGHT)
    }

    fn backbone_at(&self, pos: DVec3) -> f64 {
        let pos = pos * 0.005 + self.backbone_offset;
        self.backbone_noise.get(pos.to_array())
    }

    fn generate_block(&self, pos: DVec3, height: f64) -> bool {
        let diff = pos.y - height;
        if diff.abs() > 17. {
            return diff < 0.;
        }
        let final_value = (diff / 16.) + self.backbone_at(pos);

        final_value < 0.
    }

    pub fn generate_chunk(&self, chunk_pos: ISizeVec3) -> Chunk {
        let mut chunk = Chunk::new();
        let top_block = chunk.palette_insert(BlockData { id: location!("minecraft:grass_block"), state: "snowy=false".to_string() });
        let bottom_block = chunk.palette_insert(BlockData { id: location!("minecraft:dirt"), state: String::new(), });
        // let log_block = BlockData { id: location!("minecraft:oak_log"), state: "axis=y".to_string(), };
        // let log_block = BlockData { id: location!("minecraft:anvil"), state: "facing=east".to_string(), };
        let log_block = chunk.palette_insert(BlockData { id: location!("minecraft:cauldron"), state: String::new(), });
        // let top_block = BlockData { id: location!("kgs-debug-blocks:debug2"), state: String::new(), };
        // let bottom_block = BlockData { id: location!("kgs-debug-blocks:debug2"), state: String::new(), };
        // let log_block = BlockData { id: location!("kgs-debug-blocks:debug2"), state: String::new(), };

        let chunk_offset = chunk_pos * CHUNK_SIZE.as_isizevec3();
        for dx in 0..CHUNK_SIZE.x {
            for dz in 0..CHUNK_SIZE.z {
                let delta_pos2d = USizeVec2::new(dx, dz);
                let global_pos2d = chunk_offset.xz() + delta_pos2d.as_isizevec2();
                let global_pos2d = global_pos2d.as_dvec2();
                let height = self.height_at(global_pos2d);
                if height + 17. < chunk_offset.y as f64 {
                    continue;
                }

                let mut prev = self.generate_block(DVec3::new(global_pos2d.x, chunk_offset.y as f64 + CHUNK_SIZE.y as f64, global_pos2d.y), height);

                let mut ground: Option<usize> = None;
                for dy in (0..CHUNK_SIZE.y).rev() {
                    let delta_pos = USizeVec3::new(dx, dy, dz);
                    let global_y = chunk_offset.y.wrapping_add_unsigned(dy);
                    let global_pos = DVec3::new(global_pos2d.x, global_y as f64, global_pos2d.y);
                    if self.generate_block(global_pos, height) {
                        if prev {
                            chunk.set(ChunkBlockIndex::from_pos(delta_pos).unwrap(), bottom_block);
                        }
                        else {
                            ground = Some(dy);
                            chunk.set(ChunkBlockIndex::from_pos(delta_pos).unwrap(), top_block);
                        }
                        prev = true;
                    }
                    else {
                        prev = false;
                    }
                }

                if global_pos2d == DVec2::ZERO {
                    if let Some(g) = ground {
                        for dy in g+1..CHUNK_SIZE.y {
                            chunk.set(ChunkBlockIndex::from_pos(USizeVec3::new(dx, dy, dz)).unwrap(), log_block);
                        }
                    }
                    else if !prev {
                        for dy in 0..CHUNK_SIZE.y {
                            if dy % 2 == 0 {
                                chunk.set(ChunkBlockIndex::from_pos(USizeVec3::new(dx, dy, dz)).unwrap(), log_block);
                            }
                        }
                    }
                }
            }
        }

        chunk
    }
}
