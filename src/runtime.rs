use crate::{computed::Computed, var::Var};
use std::{cell::Cell, collections::HashSet, hash, mem, ptr, rc::Rc};

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

    pub fn computed<'a, T>(self: &Rc<Self>, compute: impl FnMut() -> T + 'a) -> Computed<'a, T> {
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
    fn record_dependency(&mut self, dependency: ComputablePtr);
    fn remove_reader(&mut self, reader: ComputablePtr);
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
    fn hashset_keeps_capacity_after_clear() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(1);
        set.insert(2);
        set.insert(3);
        set.clear();
        assert!(set.capacity() >= 3);
    }
}
