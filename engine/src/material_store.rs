use std::{ any::{Any, TypeId}, collections::{HashMap, HashSet, hash_map}, marker::PhantomData };

use genmap::{ GenMap, Handle };
use itertools::chain;
use render_graph::RenderGraph;

use crate::material::{Material, RenderGraphWrapper};
use static_assertions as sa;

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
    register: Box<dyn Send + Sync + Fn(&mut RenderGraphWrapper<'_>)>,
    registered_nodes: Option<Vec<render_graph::UntypedNodeHandle>>,
    update: Box<dyn Send + Sync + Fn(&mut RenderGraph)>,
}

#[derive(Default)]
pub struct MaterialStore {
    material_type_register_queue: HashSet<TypeId>,
    material_type_unregister_queue: HashSet<TypeId>,
    material_types_datas: HashMap<TypeId, MaterialTypeData>,

    node_unregister_queue: Vec<render_graph::UntypedNodeHandle>,
    materials: GenMap<StoredMaterial>,
}
sa::assert_impl_all!(MaterialStore: Send, Sync);

impl MaterialStore {
    fn increment_material_type<M: Material>(&mut self) {
        let type_id = TypeId::of::<M>();
        let data = match self.material_types_datas.entry(type_id) {
            hash_map::Entry::Occupied(occupied) => occupied.into_mut(),
            hash_map::Entry::Vacant(vacant) => {
                vacant.insert(MaterialTypeData {
                    refcount: 0,
                    register: Box::new(|render_graph| M::register_global(render_graph)),
                    registered_nodes: None,
                    update: Box::new(|render_graph| M::update_global(render_graph)),
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

        #[expect(clippy::iter_over_hash_type, reason = "Order (should) not matter")]
        for data in self.material_types_datas.values_mut() {
            (data.update)(render_graph);
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
