use std::mem::ManuallyDrop;

pub trait Gather {
    type Value;
    fn gather<const N: usize>(self, idxs: [usize; N]) -> [Self::Value; N];
}

// NOTE: This specilization is really not needed, removing this impl, making
// this trait only implemented for `T: Clone` to remove the specialization
// feature requirement should be easy.
impl<const S: usize, T> Gather for [T; S] {
    type Value = T;
    default fn gather<const N: usize>(self, idxs: [usize; N]) -> [Self::Value; N] {
        let mut used = [false; S];
        for idx in idxs {
            assert!(!used[idx]);
            used[idx] = true;
        }

        let mut this = self.map(|t| ManuallyDrop::new(t));
        let result = std::array::from_fn(|i| {
            // SAFETY: All indexes are checked to be unique in the step before
            unsafe { ManuallyDrop::take(&mut this[idxs[i]]) }
        });

        for i in 0..S {
            if !used[i] {
                // SAFETY: None of these indexes have been moved
                unsafe {
                    ManuallyDrop::drop(&mut this[i]);
                }
            }
        }

        result
    }
}

impl<const S: usize, T: Clone> Gather for [T; S] {
    fn gather<const N: usize>(self, idxs: [usize; N]) -> [Self::Value; N] {
        std::array::from_fn(|i| {
            self[idxs[i]].clone()
        })
    }
}
