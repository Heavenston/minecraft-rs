use std::{ any::{Any, TypeId}, collections::{HashMap, HashSet, hash_map}, marker::PhantomData, time::Instant };

use crevice::std140::AsStd140;
use glam::{ Mat4, Vec4 };
use genmap::{ GenMap, Handle };
use itertools::chain;
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

struct MaterialTypeData {
    refcount: usize,
    register: Box<dyn Fn(&mut RenderGraphWrapper<'_>)>,
    registered_nodes: Option<Vec<render_graph::UntypedNodeHandle>>,
}

pub struct World {
    pub(crate) created_at: Instant,

    pub clear_color: Vec4,
    pub camera_transform: Mat4,
    pub camera_projection: Mat4,

    material_type_register_queue: HashSet<TypeId>,
    material_type_unregister_queue: HashSet<TypeId>,
    material_types_datas: HashMap<TypeId, MaterialTypeData>,

    node_unregister_queue: Vec<render_graph::UntypedNodeHandle>,
    materials: GenMap<StoredMaterial>,
}

impl World {
    fn increment_material_type<M: Material>(&mut self) {
        let type_id = TypeId::of::<M>();
        let data = match self.material_types_datas.entry(type_id) {
            hash_map::Entry::Occupied(occupied) => occupied.into_mut(),
            hash_map::Entry::Vacant(vacant) => {
                vacant.insert(MaterialTypeData {
                    refcount: 0,
                    register: Box::new(|render_graph| M::register_global(render_graph)),
                    registered_nodes: None,
                })
            },
        };
        data.refcount += 1;
        self.material_type_unregister_queue.remove(&type_id);
        if data.registered_nodes.is_none() {
            self.material_type_register_queue.insert(type_id);
        }
    }

    fn decrement_material_type<M: Material>(&mut self) {
        let type_id = TypeId::of::<M>();
        let mut entry = match self.material_types_datas.entry(type_id) {
            hash_map::Entry::Occupied(occupied) => occupied,
            hash_map::Entry::Vacant(_) => unreachable!(),
        };
        entry.get_mut().refcount -= 1;
        if entry.get().refcount == 0 {
            self.material_type_register_queue.remove(&type_id);
            if entry.get().registered_nodes.is_none() {
                debug_assert!(!self.material_type_unregister_queue.contains(&type_id));
                entry.remove();
            }
            else {
                self.material_type_unregister_queue.insert(type_id);
            }
        }
    }

    pub fn add_material<M: Material>(&mut self, material: M) -> MaterialHandle<M> {
        self.increment_material_type::<M>();
        let handle = self.materials.insert(StoredMaterial::new(material));
        MaterialHandle {
            _material: PhantomData,
            handle,
        }
    }

    pub fn remove_material<M: Material>(&mut self, handle: MaterialHandle<M>) -> Option<M> {
        let stored = self.materials.remove(handle.handle)?;
        self.node_unregister_queue.extend(stored.registered_nodes.into_iter().flatten());

        assert_eq!(stored.material.as_ref().type_id(), TypeId::of::<M>());
        self.decrement_material_type::<M>();
        
        Some(*(stored.material as Box<dyn Any>).downcast().expect("Correct type associated with handle"))
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

        let material_types_nodes = self.material_type_unregister_queue.drain()
            .flat_map(|material_type_id| self.material_types_datas.remove(&material_type_id).and_then(|m| m.registered_nodes).expect("unregister material exists and has registered nodes"));
        let material_nodes = self.node_unregister_queue.drain(..);
        for node in chain!(material_types_nodes, material_nodes) {
            render_graph.remove_node_untyped(node);
        }

        #[expect(clippy::iter_over_hash_type, reason = "Order (should) not matter")]
        for type_id in self.material_type_register_queue.drain() {
            let data = self.material_types_datas.get_mut(&type_id).expect("materials types scheduled to registering exists");
            debug_assert!(data.registered_nodes.is_none());
            let mut wrapper = RenderGraphWrapper::new(render_graph);
            (data.register)(&mut wrapper);
            data.registered_nodes = Some(wrapper.finish());
        }

        for stored_material in self.materials.values_mut().iter_mut() {
            if stored_material.registered_nodes.is_none() {
                let mut wrapper = RenderGraphWrapper::new(render_graph);
                stored_material.material.register(&mut wrapper);
                stored_material.registered_nodes = Some(wrapper.finish());
            }
            else {
                stored_material.material.update(render_graph);
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

            material_type_register_queue: HashSet::default(),
            material_type_unregister_queue: HashSet::default(),
            material_types_datas: HashMap::default(),

            materials: GenMap::new(),
            node_unregister_queue: vec![],
        }
    }
}

#[derive(Debug, Copy, Clone, AsStd140)]
#[expect(clippy::module_name_repetitions, reason = "It is probably not in the correct module, as 'World' here has slightly different meaning")]
pub struct WorldUniform {
    pub view_projection_matrix: Mat4,
    pub time: f32,
}
