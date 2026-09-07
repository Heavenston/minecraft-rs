use std::{ any::Any, marker::PhantomData, time::Instant };

use crevice::std140::AsStd140;
use glam::{ Mat4, Vec4 };
use genmap::{ GenMap, Handle };
use render_graph::RenderGraph;

use crate::material::{Material, RenderGraphWrapper};

struct StoredMaterial {
    material: Box<dyn Material>,
    registered_nodes: Option<Vec<render_graph::UntypedNodeHandle>>,
}

impl StoredMaterial {
    #[expect(clippy::single_call_fn, reason = "Just a wrapper used once")]
    fn new<M: Material>(material: M) -> Self {
        Self {
            material: Box::new(material),
            registered_nodes: None,
        }
    }

    fn downcast_ref<M: Material>(&self) -> Option<&M> {
        (self.material.as_ref() as &dyn Any).downcast_ref()
    }

    fn downcast_mut<M: Material>(&mut self) -> Option<&mut M> {
        (self.material.as_mut() as &mut dyn Any).downcast_mut()
    }
}

pub struct MaterialHandle<M: Material> {
    _material: PhantomData<fn(M) -> M>,
    handle: Handle<StoredMaterial>,
}

impl<M: Material> std::fmt::Debug for MaterialHandle<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("MaterialHandle")
            .field(&self.handle)
            .finish()
    }
}

impl<M: Material> Copy for MaterialHandle<M> { }
impl<M: Material> Clone for MaterialHandle<M> {
    fn clone(&self) -> Self { *self }
}

pub struct World {
    pub(crate) created_at: Instant,

    pub clear_color: Vec4,
    pub camera_transform: Mat4,
    pub camera_projection: Mat4,

    materials: GenMap<StoredMaterial>,
}

impl World {
    pub fn add_material<M: Material>(&mut self, material: M) -> MaterialHandle<M> {
        let handle = self.materials.insert(StoredMaterial::new(material));
        MaterialHandle {
            _material: PhantomData,
            handle,
        }
    }

    pub fn get_material<M: Material>(&self, handle: MaterialHandle<M>) -> Option<&M> {
        Some(self.materials.get(handle.handle)?.downcast_ref().expect("Correct type associated with handle"))
    }

    pub fn get_material_mut<M: Material>(&mut self, handle: MaterialHandle<M>) -> Option<&mut M> {
        Some(self.materials.get_mut(handle.handle)?.downcast_mut().expect("Correct type associated with handle"))
    }

    pub(crate) fn update_render_graph(&mut self, render_graph: &mut RenderGraph) {
        render_graph.set_input::<crate::render_graph_nodes::RenderPassConfigResource>(crate::RenderPassConfig {
            clear_color: wgpu::Color {
                r: self.clear_color.x.into(),
                g: self.clear_color.y.into(),
                b: self.clear_color.z.into(),
                a: self.clear_color.w.into(),
            },
        });

        for m in self.materials.values_mut().iter_mut() {
            if m.registered_nodes.is_none() {
                let mut wrapper = RenderGraphWrapper::new(render_graph);
                m.material.register(&mut wrapper);
                m.registered_nodes = Some(wrapper.finish());
            }
            else {
                m.material.update(render_graph);
            }
        }
    }
}

impl Default for World {
    fn default() -> Self {
        Self {
            created_at: Instant::now(),

            clear_color: Vec4::new(0., 0., 0., 1.),
            camera_transform: Mat4::IDENTITY,
            camera_projection: Mat4::IDENTITY,

            materials: GenMap::new(),
        }
    }
}

#[derive(Debug, Copy, Clone, AsStd140)]
#[expect(clippy::module_name_repetitions, reason = "It is probably not in the correct module, as 'World' here has slightly different meaning")]
pub struct WorldUniform {
    pub view_projection_matrix: Mat4,
    pub time: f32,
}
