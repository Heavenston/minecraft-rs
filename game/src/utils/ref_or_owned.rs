use std::{ borrow::Borrow, ops::Deref };

pub trait RefOrOwned<'a, T>: Borrow<T> {
    fn into_owned(self) -> T;
}

impl<'a, T> RefOrOwned<'a, T> for &'a T
    where T: Clone,
{
    fn into_owned(self) -> T {
        self.clone()
    }
}

impl<T> RefOrOwned<'_, T> for T {
    fn into_owned(self) -> T { self }
}
