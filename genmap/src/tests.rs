use super::*;

#[test]
fn new_empty() {
    let map = GenMap::<u32>::new();
    assert!(map.values().is_empty());
}

#[test]
fn zst_new_empty() {
    let map = GenMap::<()>::new();
    assert!(map.values().is_empty());
}

#[test]
fn insert_check_slice() {
    let mut map = GenMap::<u32>::new();
    map.insert(67);
    assert_eq!(map.values(), &[67]);
}

#[test]
fn zst_insert_check_slice() {
    let mut map = GenMap::<()>::new();
    map.insert(());
    assert_eq!(map.values(), &[()]);
}

#[test]
fn insert_twice_check_slice() {
    let mut map = GenMap::<u32>::new();
    map.insert(67);
    map.insert(69);
    assert_eq!(map.values(), &[67, 69]);
}

#[test]
fn zst_insert_twice_check_slice() {
    let mut map = GenMap::<()>::new();
    map.insert(());
    map.insert(());
    assert_eq!(map.values(), &[(), ()]);
}

#[test]
fn insert_check_get() {
    let mut map = GenMap::<u32>::new();
    let handle = map.insert(67);
    assert_eq!(map.get(handle), Some(&67));
}

#[test]
fn zst_insert_check_get() {
    let mut map = GenMap::<()>::new();
    let handle = map.insert(());
    assert_eq!(map.get(handle), Some(&()));
}

#[test]
fn insert_twice_check_get() {
    let mut map = GenMap::<u32>::new();
    let handle1 = map.insert(67);
    let handle2 = map.insert(69);
    assert_eq!(map.get(handle1), Some(&67));
    assert_eq!(map.get(handle2), Some(&69));
}

#[test]
fn insert_twice_check_different_handles() {
    let mut map = GenMap::<u32>::new();
    let handle1 = map.insert(67);
    let handle2 = map.insert(69);
    assert_ne!(handle1, handle2);
}

#[test]
fn zst_insert_twice_check_get() {
    let mut map = GenMap::<()>::new();
    let handle1 = map.insert(());
    let handle2 = map.insert(());
    assert_eq!(map.get(handle1), Some(&()));
    assert_eq!(map.get(handle2), Some(&()));
}

#[test]
fn insert_remove_check_return() {
    let mut map = GenMap::<u32>::new();
    let handle = map.insert(67);
    assert_eq!(map.remove(handle), Some(67));
}

#[test]
fn insert_remove_twice_check_return() {
    let mut map = GenMap::<u32>::new();
    let handle = map.insert(67);
    map.remove(handle);
    assert_eq!(map.remove(handle), None);
}

#[test]
fn insert_twice_remove1_check_return() {
    let mut map = GenMap::<u32>::new();
    let handle1 = map.insert(67);
    let _handle2 = map.insert(69);
    assert_eq!(map.remove(handle1), Some(67));
}

#[test]
fn insert_twice_remove2_check_return() {
    let mut map = GenMap::<u32>::new();
    let _handle1 = map.insert(67);
    let handle2 = map.insert(69);
    assert_eq!(map.remove(handle2), Some(69));
}

#[test]
fn insert_remove_insert() {
    let mut map = GenMap::<u32>::new();
    let handle1 = map.insert(67);
    map.remove(handle1);
    let handle2 = map.insert(69);
    assert_ne!(handle1, handle2);
    assert_eq!(map.get(handle1), None);
    assert_eq!(map.get(handle2), Some(&69));
    assert_eq!(map.values(), &[69]);
}

#[test]
fn insert_remove_insert_remove() {
    let mut map = GenMap::<u32>::new();
    let handle1 = map.insert(67);
    map.remove(handle1);
    let handle2 = map.insert(69);
    map.remove(handle2);
    assert_eq!(map.get(handle1), None);
    assert_eq!(map.get(handle2), None);
    assert_eq!(map.values(), &[]);
}

#[test]
fn insert_twice_remove1_insert() {
    let mut map = GenMap::<u32>::new();
    let handle1 = map.insert(67);
    let handle2 = map.insert(69);
    map.remove(handle1);
    let handle3 = map.insert(420);
    assert_ne!(handle1, handle2);
    assert_ne!(handle1, handle3);
    assert_ne!(handle2, handle3);
    assert_eq!(map.get(handle1), None);
    assert_eq!(map.get(handle2), Some(&69));
    assert_eq!(map.get(handle3), Some(&420));
    assert_eq!(map.values(), &[69, 420]);
}

#[test]
fn insert_twice_remove2_insert() {
    let mut map = GenMap::<u32>::new();
    let handle1 = map.insert(67);
    let handle2 = map.insert(69);
    map.remove(handle2);
    let handle3 = map.insert(420);
    assert_ne!(handle1, handle2);
    assert_ne!(handle1, handle3);
    assert_ne!(handle2, handle3);
    assert_eq!(map.get(handle1), Some(&67));
    assert_eq!(map.get(handle2), None);
    assert_eq!(map.get(handle3), Some(&420));
    assert_eq!(map.values(), &[67, 420]);
}

#[test]
fn insert_twice_remove_twice_insert() {
    let mut map = GenMap::<u32>::new();
    let handle1 = map.insert(67);
    let handle2 = map.insert(69);
    map.remove(handle1);
    map.remove(handle2);
    let handle3 = map.insert(420);
    assert_ne!(handle1, handle2);
    assert_ne!(handle1, handle3);
    assert_ne!(handle2, handle3);
    assert_eq!(map.get(handle1), None);
    assert_eq!(map.get(handle2), None);
    assert_eq!(map.get(handle3), Some(&420));
    assert_eq!(map.values(), &[420]);
}
