pub mod graph;

use std::{any::TypeId, collections::HashSet};

use parking_lot::{MappedRwLockReadGuard, MappedRwLockWriteGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};
use typemap::TypeMap;

#[derive(Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum RenderStage {
    None,
    Background,
    Content3D,
    Content2D,
    UI,
}

pub trait ContructibleMiddleware {
    fn new(world: &mut RenderWorld) -> (RenderStage, Self);
}

pub trait RenderMiddleware: std::any::Any {
    fn prepare(&mut self, world: &RenderWorld) { let _ = world; }
    fn render(&mut self, render_pass: &mut wgpu::RenderPass<'_>) { let _ = render_pass; }
}

trait RwLockedMiddleware: std::any::Any {
    #[expect(dead_code)]
    fn dyn_read(&self) -> MappedRwLockReadGuard<'_, dyn RenderMiddleware>;
    fn dyn_write(&self) -> MappedRwLockWriteGuard<'_, dyn RenderMiddleware>;
}

impl<M: std::any::Any + RenderMiddleware> RwLockedMiddleware for RwLock<M> {
    fn dyn_read(&self) -> MappedRwLockReadGuard<'_, dyn RenderMiddleware> {
        RwLockReadGuard::map(RwLock::read(self), |p| p as &dyn RenderMiddleware)
    }

    fn dyn_write(&self) -> MappedRwLockWriteGuard<'_, dyn RenderMiddleware> {
        RwLockWriteGuard::<'_, M>::map(RwLock::write(self), |p: &mut M| p as &mut dyn RenderMiddleware)
    }
}
typemap::impl_dyn_trait!(RwLockedMiddleware);

pub struct RenderWorld {
    device: wgpu::Device,
    queue: wgpu::Queue,
    /// Used to detect recursive use of push_middleware with the same middleware
    recursive_use_middleware: HashSet<TypeId>,
    middlewares: TypeMap<dyn RwLockedMiddleware>,
    middlewares_order: Vec<(RenderStage, TypeId)>,

    surface_format: wgpu::TextureFormat,
    width: u32,
    height: u32,
}

impl RenderWorld {
    pub(crate) fn new(device: wgpu::Device, queue: wgpu::Queue, surface_format: wgpu::TextureFormat) -> Self {
        Self {
            device,
            queue,
            recursive_use_middleware: HashSet::new(),
            middlewares: TypeMap::new(),
            middlewares_order: vec![],

            surface_format,
            width: 0,
            height: 0,
        }
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    pub fn render_target_format(&self) -> wgpu::TextureFormat {
        self.surface_format
    }

    pub(crate) fn set_render_target_size(&mut self, (width, height): (u32, u32)) {
        self.width = width;
        self.height = height;
    }

    pub fn render_target_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn register_middleware_stage(middlewares_order: &mut Vec<(RenderStage, TypeId)>, stage: RenderStage, tid: TypeId) {
        debug_assert!(!middlewares_order.iter().any(|&(_, tid2)| tid == tid2), "Cannot register a middleware twice");

        let idx = middlewares_order.partition_point(|&(s, _)| s <= stage);
        middlewares_order.insert(idx, (stage, tid));
        debug_assert!(middlewares_order.is_sorted_by_key(|&(s, _)| s));
    }

    pub fn push_middleware<M: RenderMiddleware>(
        &mut self,
        stage: RenderStage,
        factory: impl FnOnce(&RenderWorld) -> M,
    ) {
        assert!(!self.middlewares.has::<RwLock<M>>(), "Attempted to push middleware {} twice", std::any::type_name::<M>());
        self.middlewares.insert(Box::new(RwLock::new(factory(self))));
        debug_assert!(self.middlewares.has::<RwLock<M>>());
        Self::register_middleware_stage(&mut self.middlewares_order, stage, TypeId::of::<RwLock<M>>());
    }

    pub fn use_middleware<M: RenderMiddleware + ContructibleMiddleware>(&mut self) -> RwLockWriteGuard<'_, M> {
        if let Some(p) = self.middlewares.get::<RwLock<M>>() {
            p.write()
        }
        else {
            let tid = TypeId::of::<RwLock<M>>();
            if !self.recursive_use_middleware.insert(tid) {
                panic!("Attempted to recursively create middlewase {}", std::any::type_name::<M>());
            }
            let (stage, middleware) = M::new(self);
            self.recursive_use_middleware.remove(&tid);
            Self::register_middleware_stage(&mut self.middlewares_order, stage, TypeId::of::<RwLock<M>>());
            self.middlewares.upsert::<RwLock<M>>(Box::new(RwLock::new(middleware))).write()
        }
    }

    pub fn get_middleware_mut<M: RenderMiddleware>(&self) -> Option<RwLockWriteGuard<'_, M>> {
        self.middlewares.get::<RwLock<M>>().map(RwLock::write)
    }

    pub fn get_middleware<M: RenderMiddleware>(&self) -> Option<RwLockReadGuard<'_, M>> {
        self.middlewares.get::<RwLock<M>>().map(RwLock::read)
    }

    pub fn remove_middleware<M: RenderMiddleware>(&mut self) -> Option<M> {
        let tid = TypeId::of::<M>();
        if let Some(idx) = self.middlewares_order.iter().position(|&(_, tid2)| tid == tid2) {
            self.middlewares_order.remove(idx);
        }
        self.middlewares.remove::<RwLock<M>>().map(|p| *p).map(RwLock::into_inner)
    }

    pub(crate) fn iter_middlewares_in_order(&self, stage: RenderStage) -> impl Iterator<Item = MappedRwLockWriteGuard<'_, dyn RenderMiddleware>> + ExactSizeIterator {
        let start = self.middlewares_order.partition_point(|&(stage2, _)| stage2 < stage);
        let end = self.middlewares_order.partition_point(|&(stage2, _)| stage2 <= stage);
        self.middlewares_order[start..end].iter()
            .map(|(_, id)| self.middlewares.get_dyn(*id).expect("Exists"))
            .map(|p| p.dyn_write())
    }
}
