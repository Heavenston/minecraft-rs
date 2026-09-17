use std::{ any::{Any, TypeId}, collections::{HashMap, HashSet, hash_map}, marker::PhantomData };

use genmap::{ GenMap, Handle };
use itertools::chain;
use render_graph::RenderGraph;

use crate::material::{GlobalMaterial, Material, RenderGraphWrapper};
use static_assertions as sa;

pub struct GlobalMaterialTuple<T>(PhantomData<fn(T) -> T>);
impl<T> GlobalMaterialTuple<T> {
    pub fn new() -> Self {
        Self::default()
    }
}
impl<T> Default for GlobalMaterialTuple<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}
impl<T> Copy for GlobalMaterialTuple<T> { }
impl<T> Clone for GlobalMaterialTuple<T> {
    fn clone(&self) -> Self { *self }
}

trait SealedGlobalMaterialList: Copy {
    fn increment(self, store: &mut MaterialStore);
    fn decrement(self, store: &mut MaterialStore);
}
macro_rules! impl_global_material_list {
    ($($T:ident),*) => {
        impl<$($T: GlobalMaterial),*> SealedGlobalMaterialList for GlobalMaterialTuple<($($T,)*)> {
            fn increment(self, _store: &mut MaterialStore) {
                $(_store.increment_global_material::<$T>();)*
            }

            fn decrement(self, _store: &mut MaterialStore) {
                $(_store.decrement_global_material::<$T>();)*
            }
        }
    };
}
variadics_please::all_tuples!(impl_global_material_list, 0, 15, T);

#[expect(private_bounds, reason = "Sealed trait")]
pub trait GlobalMaterialList: SealedGlobalMaterialList { }
impl<T: SealedGlobalMaterialList> GlobalMaterialList for T { }

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

enum GlobalMaterialState {
    ToRegister(Box<dyn Send + Sync + Fn(&mut RenderGraphWrapper<'_>) -> Box<dyn GlobalMaterial>>),
    Registered {
        registered_nodes: Vec<render_graph::UntypedNodeHandle>,
        value: Box<dyn GlobalMaterial>,
    },
}

struct GlobalMaterialData {
    refcount: usize,
    state: GlobalMaterialState,
}

#[derive(Default)]
pub struct MaterialStore {
    material_type_register_queue: HashSet<TypeId>,
    material_type_unregister_queue: HashSet<TypeId>,
    material_types_datas: HashMap<TypeId, GlobalMaterialData>,

    node_unregister_queue: Vec<render_graph::UntypedNodeHandle>,
    materials: GenMap<StoredMaterial>,
}
sa::assert_impl_all!(MaterialStore: Send, Sync);

impl MaterialStore {
    fn increment_global_material<M: GlobalMaterial>(&mut self) {
        let type_id = TypeId::of::<M>();
        let data = match self.material_types_datas.entry(type_id) {
            hash_map::Entry::Occupied(occupied) => occupied.into_mut(),
            hash_map::Entry::Vacant(vacant) => {
                vacant.insert(GlobalMaterialData {
                    refcount: 0,
                    state: GlobalMaterialState::ToRegister(Box::new(|render_graph| Box::new(M::register(render_graph))))
                })
            },
        };
        data.refcount += 1;
        self.material_type_unregister_queue.remove(&type_id);
        if matches!(data.state, GlobalMaterialState::ToRegister(_)) {
            self.material_type_register_queue.insert(type_id);
        }
    }

    fn decrement_global_material<M: GlobalMaterial>(&mut self) {
        let type_id = TypeId::of::<M>();
        let mut entry = match self.material_types_datas.entry(type_id) {
            hash_map::Entry::Occupied(occupied) => occupied,
            hash_map::Entry::Vacant(_) => unreachable!(),
        };
        entry.get_mut().refcount -= 1;
        if entry.get().refcount == 0 {
            self.material_type_register_queue.remove(&type_id);
            if matches!(entry.get().state, GlobalMaterialState::ToRegister(_)) {
                debug_assert!(!self.material_type_unregister_queue.contains(&type_id));
                entry.remove();
            }
            else {
                self.material_type_unregister_queue.insert(type_id);
            }
        }
    }

    pub fn add_material<M: Material>(&mut self, material: M) -> MaterialHandle<M> {
        M::global_materials().increment(self);
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
        M::global_materials().decrement(self);
        
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
            .flat_map(|material_type_id| self.material_types_datas.remove(&material_type_id).map(|m| match m.state {
                GlobalMaterialState::ToRegister(_) => unreachable!("global materials to be unregistered should be registered"),
                GlobalMaterialState::Registered { registered_nodes, .. } => registered_nodes,
            }).expect("global materials to be unregistered should exist"));
        let material_nodes = self.node_unregister_queue.drain(..);
        for node in chain!(material_types_nodes, material_nodes) {
            render_graph.remove_node_untyped(node);
        }

        #[expect(clippy::iter_over_hash_type, reason = "Order (should) not matter")]
        for type_id in self.material_type_register_queue.drain() {
            let data = self.material_types_datas.get_mut(&type_id).expect("materials types scheduled to registering exists");
            let register = match &data.state {
                GlobalMaterialState::ToRegister(register) => register,
                GlobalMaterialState::Registered { .. } => unreachable!("global materials to register should not be already registered"),
            };
            let mut wrapper = RenderGraphWrapper::new(render_graph);
            let value = register(&mut wrapper);
            data.state = GlobalMaterialState::Registered {
                registered_nodes: wrapper.finish(),
                value,
            };
        }

        for stored_material in self.materials.values_mut().iter_mut() {
            if stored_material.registered_nodes.is_none() {
                let mut wrapper = RenderGraphWrapper::new(render_graph);
                stored_material.material.register(&mut wrapper);
                stored_material.registered_nodes = Some(wrapper.finish());
            }
        }

        #[expect(clippy::iter_over_hash_type, reason = "Order (should) not matter")]
        for data in self.material_types_datas.values_mut() {
            if let GlobalMaterialState::Registered { value, .. } = &mut data.state {
                value.update(render_graph);
            }
        }

        for stored_material in self.materials.values_mut().iter_mut() {
            stored_material.material.update(render_graph);
        }
    }
}
