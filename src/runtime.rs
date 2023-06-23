use crate::value::Value;
use std::{
    cell::{Cell, RefCell, RefMut},
    collections::HashSet,
    hash, ptr,
    rc::Rc,
};

pub struct Runtime {
    current: Cell<Option<ComputablePtr>>,
}

impl Runtime {
    pub fn new() -> Rc<Runtime> {
        Rc::new(Runtime {
            current: None.into(),
        })
    }

    pub fn var<T>(self: &Rc<Self>, value: T) -> Value<T> {
        Value::new_var(self, value)
    }

    pub fn computed<T>(self: &Rc<Self>, compute: impl FnMut() -> T + 'static) -> Value<T> {
        Value::new_computed(self, compute)
    }

    /// Create a computed value that memoizes its result.
    ///
    /// The `key` function is invoked to determine if the value should be recomputed. If the key
    /// changes, the `compute` function is called. If the key is the same, the previous value is
    /// returned.
    ///
    /// `K` needs to implement `PartialEq` so that the key can be compared to the previous key and
    /// `T` to implement `Clone` so that the previous value can be returned.
    ///
    /// `T` should also be cheap to clone, e.g `Rc`, since it is stored two times in the node. In
    /// the cache, and as the result in the computed node.
    ///
    /// Even though tracked, dependencies that were invalidated and tracked _only_ in the compute
    /// function may not cause the value to be recomputed when the key stays the same. `compute`
    /// should therefore not resolve _any_ node values belonging to the same runtime. This might
    /// even be tested for in future updates.
    pub fn memo<K, T>(
        self: &Rc<Self>,
        key: impl Fn() -> K + 'static,
        mut compute: impl FnMut(&K) -> T + 'static,
    ) -> Value<T>
    where
        K: PartialEq + 'static,
        T: Clone,
    {
        let mut prev: Option<(K, T)> = None;
        Value::new_computed(self, move || {
            let key = key();
            if let Some((prev_key, prev_value)) = &prev {
                if key == *prev_key {
                    return prev_value.clone();
                }
            }
            let value = compute(&key);
            prev = Some((key, value.clone()));
            value
        })
    }

    pub(crate) fn eval(&self, current: ComputablePtr, f: impl FnOnce()) {
        let prev = self.current.get();
        self.current.set(Some(current));
        f();
        self.current.set(prev);
    }

    pub(crate) fn current(&self) -> Option<ComputablePtr> {
        self.current.get()
    }
}

pub trait Computable {
    fn invalidate(&mut self);
    fn track_read_from(&mut self, from: Rc<dyn RefCellComputable>);
    fn remove_reader(&mut self, reader: ComputablePtr);
}

pub trait RefCellComputable {
    fn as_ptr(&self) -> ComputablePtr;

    fn borrow_mut(&self) -> RefMut<dyn Computable>;

    #[allow(clippy::mut_from_ref)]
    unsafe fn as_mut(&self) -> &mut dyn Computable;
}

impl<T> RefCellComputable for RefCell<T>
where
    T: Computable,
{
    fn as_ptr(&self) -> ComputablePtr {
        ComputablePtr::new(unsafe { &*RefCell::as_ptr(self) })
    }

    fn borrow_mut(&self) -> RefMut<dyn Computable> {
        RefMut::map(self.borrow_mut(), |t| t as &mut dyn Computable)
    }

    unsafe fn as_mut(&self) -> &mut dyn Computable {
        (&mut *RefCell::as_ptr(self)) as &mut dyn Computable
    }
}

#[derive(Clone)]
pub(crate) struct RefCellComputableHandle(pub Rc<dyn RefCellComputable>);

impl PartialEq for RefCellComputableHandle {
    fn eq(&self, other: &Self) -> bool {
        // Can't compare trait ptrs using Rc::ptr_eq.
        let ptr_self = self.0.as_ptr();
        let ptr_other = other.0.as_ptr();
        ptr_self == ptr_other
    }
}

impl Eq for RefCellComputableHandle {}

impl hash::Hash for RefCellComputableHandle {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        Rc::as_ptr(&self.0).hash(state)
    }
}

impl RefCellComputableHandle {
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn as_mut(&self) -> &mut dyn Computable {
        self.0.as_mut()
    }
}

/// This holds a pointer to a computable by preserving identity (trait objects can't be compared
/// equality because their vtable pointer is not stable).
#[repr(transparent)]
#[derive(Clone, Copy, Eq)]
pub struct ComputablePtr(ptr::NonNull<dyn Computable>);

impl PartialEq for ComputablePtr {
    fn eq(&self, other: &Self) -> bool {
        self.0.cast::<()>() == other.0.cast()
    }
}

impl hash::Hash for ComputablePtr {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.0.cast::<()>().hash(state)
    }
}

impl ComputablePtr {
    pub fn new(computable: &dyn Computable) -> Self {
        ComputablePtr(unsafe {
            ptr::NonNull::new_unchecked(computable as *const dyn Computable as *mut dyn Computable)
        })
    }

    pub unsafe fn as_mut(&mut self) -> &mut dyn Computable {
        self.0.as_mut()
    }
}

pub(crate) type Readers = HashSet<ComputablePtr>;
pub(crate) type Trace = Vec<RefCellComputableHandle>;

#[cfg(test)]
mod tests {
    #[test]
    fn hash_set_keeps_capacity_after_clear() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(1);
        set.insert(2);
        set.insert(3);
        set.clear();
        assert!(set.capacity() >= 3);
    }
}
