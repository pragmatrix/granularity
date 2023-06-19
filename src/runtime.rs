use crate::{computed::Computed, var::Var};
use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
    hash, mem, ptr,
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

    pub fn var<T>(self: &Rc<Self>, value: T) -> Var<T> {
        Var::new(self, value)
    }

    pub fn computed<T>(self: &Rc<Self>, compute: impl FnMut() -> T + 'static) -> Computed<T> {
        Computed::new(self, compute)
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
    fn record_dependency(&mut self, dependency: Rc<dyn RefCellComputable>);
    fn remove_reader(&mut self, reader: ComputablePtr);
}

pub trait RefCellComputable {
    fn as_ptr(&self) -> ComputablePtr;
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

pub type Readers = HashSet<ComputablePtr>;

// Invalidate all readers (Invoking `invalidate()` on readers may call `remove_reader()` on the
// `Computable` invoking the function, so don't touch `readers` while iterating)
pub fn invalidate_readers(readers_: &mut Readers) {
    let mut readers = mem::take(readers_);
    for reader in &readers {
        unsafe { reader.clone().as_mut() }.invalidate();
    }
    readers.clear();
    // Readers are not allowed to be changed while invalidation runs.
    debug_assert!(readers_.is_empty());
    // Put the empty readers back, to keep the allocated capacity for this var.
    *readers_ = readers;
}

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
