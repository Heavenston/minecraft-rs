use static_assertions as ca;

use crate::chunk::CHUNK_SIZE;

ca::const_assert!(CHUNK_SIZE.x.is_power_of_two());
ca::const_assert!(CHUNK_SIZE.y.is_power_of_two());
ca::const_assert!(CHUNK_SIZE.z.is_power_of_two());
// 15 Bits required to store local offset into chunk
ca::const_assert!((CHUNK_SIZE.x.ilog2() + CHUNK_SIZE.y.ilog2() + CHUNK_SIZE.z.ilog2()) == 15);

/// First 15 bits are offset into the chunk
pub struct FaceInstanceData(u16);
pub struct VertexData(u8);


