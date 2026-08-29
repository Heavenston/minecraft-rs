use std::{ any::{ Any, TypeId }, collections::HashMap };

pub trait DynTrait: Any + 'static {
    fn as_dyn_any(&self) -> &dyn Any;
    fn as_dyn_any_mut(&mut self) -> &mut dyn Any;
    fn into_box_dyn(self: Box<Self>) -> Box<dyn Any>;
}

pub trait DynTraitFrom<T: ?Sized> {
    fn dyn_trait_from_box(from: Box<T>) -> Box<Self>;
}

pub trait DynTraitInto<T: ?Sized> {
    fn dyn_trait_into_box(from: Box<Self>) -> Box<T>;
}

impl<A: ?Sized, B: ?Sized> DynTraitInto<B> for A
    where B: DynTraitFrom<A>,
{
    fn dyn_trait_into_box(from: Box<Self>) -> Box<B> {
        B::dyn_trait_from_box(from)
    }
}

#[macro_export]
macro_rules! impl_dyn_trait {
    ($($trait: tt)*) => {
        impl $crate::DynTrait for dyn $($trait)* {
            fn as_dyn_any(&self) -> &dyn ::std::any::Any { self }
            fn as_dyn_any_mut(&mut self) -> &mut dyn ::std::any::Any { self }
            fn into_box_dyn(self: Box<Self>) -> Box<dyn ::std::any::Any> { self }
        }

        impl<T: $($trait)*> $crate::DynTraitFrom<T> for dyn $($trait)* {
            fn dyn_trait_from_box(from: Box<T>) -> Box<dyn $($trait)*> {
                from as Box<_>
            }
        }
    };
}
impl_dyn_trait!(Any);
impl_dyn_trait!(Any + Send);
impl_dyn_trait!(Any + Send + Sync);
impl_dyn_trait!(Any + Sync);


pub struct TypeMap<T: ?Sized + Any = dyn Any> {
    values: HashMap<TypeId, Box<T>>,
}

impl<T: ?Sized + DynTrait> TypeMap<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> + ExactSizeIterator {
        self.values.values().map(|p| &**p)
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> + ExactSizeIterator {
        self.values.values_mut().map(|p| &mut **p)
    }

    pub fn reserve(&mut self, additional: usize) {
        self.values.reserve(additional);
    }

    pub fn insert(&mut self, value: Box<T>) -> Option<Box<T>> {
        let type_id = <T as Any>::type_id(&*value);
        self.values.insert(type_id, value)
    }

    pub fn upsert<U: Any>(&mut self, value: Box<U>) -> &mut U
        where T: DynTraitFrom<U>,
    {
        let type_id = TypeId::of::<U>();
        let boxed = self.values.entry(type_id).insert_entry(T::dyn_trait_from_box(value)).into_mut();
        T::as_dyn_any_mut(boxed).downcast_mut().expect("matching type id")
    }

    pub fn has<U: Any>(&self) -> bool {
        self.values.contains_key(&TypeId::of::<U>())
    }

    pub fn get_dyn(&self, type_id: TypeId) -> Option<&T> {
        self.values.get(&type_id).map(|t| &**t)
    }

    pub fn get_dyn_mut(&mut self, type_id: TypeId) -> Option<&mut T> {
        self.values.get_mut(&type_id).map(|t| &mut **t)
    }

    pub fn get<U: Any>(&self) -> Option<&U> {
        self.values.get(&TypeId::of::<U>())
            .map(|t| { t.as_dyn_any().downcast_ref::<U>().expect("matching type id") })
    }

    pub fn get_mut<U: Any>(&mut self) -> Option<&mut U> {
        self.values.get_mut(&TypeId::of::<U>())
            .map(|t| { t.as_dyn_any_mut().downcast_mut::<U>().expect("matching type id") })
    }

    pub fn remove<U: Any>(&mut self) -> Option<Box<U>> {
        self.values.remove(&TypeId::of::<U>())
            .map(|p| p.into_box_dyn().downcast().expect("matching type id"))
    }

    pub fn get_or_insert_with<U: Any>(&mut self, factory: impl FnOnce() -> Box<U>) -> &mut U
        where T: DynTraitFrom<U>,
    {
        let entry = self.values.entry(TypeId::of::<U>());
        let boxed = entry.or_insert_with(|| T::dyn_trait_from_box(factory()));
        boxed.as_dyn_any_mut().downcast_mut::<U>().expect("matching type id")
    }

    pub fn get_or_insert<U: Any>(&mut self, value: Box<U>) -> &mut U
        where T: DynTraitFrom<U>,
    {
        self.get_or_insert_with(move || value)
    }

    pub fn get_or_default<U: Any + Default>(&mut self) -> &mut U
        where T: DynTraitFrom<U>,
    {
        self.get_or_insert_with(Box::<U>::default)
    }
}

impl<T: ?Sized + Any> Default for TypeMap<T> {
    fn default() -> Self {
        Self { values: Default::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::TypeMap;
    use std::{ any::Any, assert_matches };

    #[test]
    fn test_any_empty() {
        let map = TypeMap::<dyn Any>::new();

        assert_eq!(map.len(), 0);
    }

    #[test]
    fn test_any_one_val() {
        let mut map = TypeMap::<dyn Any>::new();
        map.insert(Box::new(152u32));

        assert_eq!(map.len(), 1);
        assert_eq!(map.get::<u32>(), Some(&152));
    }

    #[test]
    fn test_any_one_val_removed() {
        let mut map = TypeMap::<dyn Any>::new();
        map.insert(Box::new(80000u32));
        assert_matches!(map.remove::<u32>(), Some(_));

        assert_eq!(map.len(), 0);
        assert_eq!(map.get::<u32>(), None);
    }

    #[test]
    fn test_any_two_val() {
        let mut map = TypeMap::<dyn Any>::new();
        map.insert(Box::new(8412u32));
        map.insert(Box::new(111u64));
        assert_eq!(map.len(), 2);
        assert_eq!(map.get::<u32>(), Some(&8412u32));
        assert_eq!(map.get::<u64>(), Some(&111u64));
    }

    #[test]
    fn test_any_two_val_removed_one_1() {
        let mut map = TypeMap::<dyn Any>::new();
        map.insert(Box::new(8412u32));
        map.insert(Box::new(111u64));
        assert_matches!(map.remove::<u32>(), Some(_));
        assert_eq!(map.len(), 1);
        assert_eq!(map.get::<u32>(), None);
        assert_eq!(map.get::<u64>(), Some(&111u64));
    }

    #[test]
    fn test_any_two_val_removed_one_2() {
        let mut map = TypeMap::<dyn Any>::new();
        map.insert(Box::new(8412u32));
        map.insert(Box::new(111u64));
        assert_matches!(map.remove::<u64>(), Some(_));
        assert_eq!(map.len(), 1);
        assert_eq!(map.get::<u32>(), Some(&8412u32));
        assert_eq!(map.get::<u64>(), None);
    }

    #[test]
    fn test_any_two_val_removed_one_3() {
        let mut map = TypeMap::<dyn Any>::new();
        map.insert(Box::new(8412u32));
        assert_matches!(map.remove::<u32>(), Some(_));
        map.insert(Box::new(111u64));
        assert_eq!(map.len(), 1);
        assert_eq!(map.get::<u32>(), None);
        assert_eq!(map.get::<u64>(), Some(&111u64));
    }

    #[test]
    fn test_any_one_val_remove_other() {
        let mut map = TypeMap::<dyn Any>::new();
        map.insert(Box::new(8412u32));
        assert_matches!(map.remove::<u64>(), None);
        assert_eq!(map.len(), 1);
        assert_eq!(map.get::<u32>(), Some(&8412u32));
        assert_eq!(map.get::<u64>(), None);
    }

    #[test]
    fn test_any_get_or_insert() {
        let mut map = TypeMap::<dyn Any>::new();
        map.get_or_insert_with::<u32>(|| Box::new(420u32));
        assert_eq!(map.len(), 1);
        assert_matches!(map.get::<u32>(), Some(420));
        assert_matches!(map.get::<u64>(), None);
    }

    trait MyCoolTrait: Any { }
    super::impl_dyn_trait!(MyCoolTrait);

    #[derive(Debug)]
    struct A;
    impl MyCoolTrait for A { }
    #[derive(Debug)]
    struct B;
    impl MyCoolTrait for B { }
    #[derive(Debug)]
    struct C;
    impl MyCoolTrait for C { }

    #[test]
    fn test_trait() {
        let map = TypeMap::<dyn MyCoolTrait>::new();
        assert_eq!(map.len(), 0);
    }

    #[test]
    fn test_trait_one_val() {
        let mut map = TypeMap::<dyn MyCoolTrait>::new();
        map.insert(Box::new(A));
        assert_eq!(map.len(), 1);
        assert_matches!(map.get::<A>(), Some(A));
        assert_matches!(map.get::<B>(), None);
        assert_matches!(map.get::<C>(), None);
    }

    #[test]
    fn test_trait_two_val() {
        let mut map = TypeMap::<dyn MyCoolTrait>::new();
        map.insert(Box::new(A));
        map.insert(Box::new(B));
        assert_eq!(map.len(), 2);
        assert_matches!(map.get::<A>(), Some(A));
        assert_matches!(map.get::<B>(), Some(B));
        assert_matches!(map.get::<C>(), None);
    }

    #[test]
    fn test_trait_three_val() {
        let mut map = TypeMap::<dyn MyCoolTrait>::new();
        map.insert(Box::new(A));
        map.insert(Box::new(B));
        map.insert(Box::new(C));
        assert_eq!(map.len(), 3);
        assert_matches!(map.get::<A>(), Some(A));
        assert_matches!(map.get::<B>(), Some(B));
        assert_matches!(map.get::<C>(), Some(C));
    }

    #[test]
    fn test_trait_three_val_remove_one() {
        let mut map = TypeMap::<dyn MyCoolTrait>::new();
        map.insert(Box::new(A));
        map.insert(Box::new(B));
        map.insert(Box::new(C));
        map.remove::<B>();
        assert_eq!(map.len(), 2);
        assert_matches!(map.get::<A>(), Some(A));
        assert_matches!(map.get::<B>(), None);
        assert_matches!(map.get::<C>(), Some(C));
    }

    #[test]
    fn test_trait_get_or_insert() {
        let mut map = TypeMap::<dyn MyCoolTrait>::new();
        map.get_or_insert_with::<A>(|| Box::new(A));
        assert_eq!(map.len(), 1);
        assert_matches!(map.get::<A>(), Some(A));
        assert_matches!(map.get::<B>(), None);
        assert_matches!(map.get::<C>(), None);
    }
}
