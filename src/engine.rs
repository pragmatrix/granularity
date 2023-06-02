use std::cell::{Cell, Ref};
use std::ops::DerefMut;
use std::ptr;
use std::{cell::RefCell, collections::HashSet, rc::Rc};
use std::{hash, mem};

pub struct Engine {
    current: Cell<Option<ComputablePtr>>,
}

impl Engine {
    pub fn new() -> Rc<Engine> {
        Rc::new(Engine {
            current: None.into(),
        })
    }

    pub fn var<T>(self: &Rc<Self>, value: T) -> Var<T>
    where
        T: Clone,
    {
        let inner = VarInner {
            engine: self.clone(),
            value: None,
            compute: Box::new(move || value.clone()),
            readers: HashSet::new(),
            dependencies: HashSet::new(),
        };
        Var {
            inner: Rc::new(RefCell::new(inner)),
        }
    }

    pub fn computed<T>(self: &Rc<Self>, compute: impl Fn() -> T + 'static) -> Var<T> {
        let inner = VarInner {
            engine: self.clone(),
            value: None,
            compute: Box::new(compute),
            readers: HashSet::new(),
            dependencies: HashSet::new(),
        };

        Var {
            inner: Rc::new(RefCell::new(inner)),
        }
    }

    fn eval(&self, current: ComputablePtr, f: impl FnOnce()) {
        let prev = self.current.get();
        self.current.set(Some(current));
        f();
        self.current.set(prev);
    }
}

/// A cheap to clone variable.
#[derive(Clone)]
pub struct Var<T: 'static> {
    inner: Rc<RefCell<VarInner<T>>>,
}

impl<T> Var<T> {
    pub fn set(&mut self, value: T)
    where
        T: Clone,
    {
        let mut inner = self.inner.borrow_mut();
        inner.compute = Box::new(move || value.clone());
        inner.invalidate();
    }
}

impl<T> Var<T> {
    pub fn get(&self) -> Ref<T> {
        // Force before adding a reader, because we can't add readers on an invalid value.
        let inner_ptr;

        {
            let mut inner = self.inner.borrow_mut();
            let inner = inner.deref_mut();
            inner.force();
            inner_ptr = inner.as_ptr();
        }

        // Add the current reader.
        {
            let reader = self.inner.borrow().engine.current.get();
            if let Some(mut reader) = reader {
                self.inner.borrow_mut().readers.insert(reader);

                let reader = unsafe { reader.as_mut() };
                reader.record_dependency(inner_ptr);
            }
        }

        let r = self.inner.borrow();
        Ref::map(r, |r| r.value.as_ref().unwrap())
    }
}

struct VarInner<T: 'static> {
    engine: Rc<Engine>,
    value: Option<T>,
    compute: Box<dyn Fn() -> T>,
    // Readers are cleared when we invalidate.
    readers: HashSet<ComputablePtr>,
    // Deps are cleared on invalidation, too. Rc is used to hold dependencies in memory.
    dependencies: HashSet<ComputablePtr>,
}

impl<T: 'static> VarInner<T> {
    pub fn force(&mut self) -> &T {
        if self.value.is_none() {
            // Readers must be empty when recomputing.
            assert!(self.readers.is_empty());
            let self_ptr = self.as_ptr();
            self.engine.eval(self_ptr, || {
                self.value = Some((self.compute)());
            });
        }
        self.value.as_ref().unwrap()
    }

    fn as_ptr(&self) -> ComputablePtr {
        ComputablePtr::new(self)
    }
}

trait Computable {
    fn invalidate(&mut self);
    fn record_dependency(&mut self, dependency: ComputablePtr);
    fn remove_reader(&mut self, reader: ComputablePtr);
}

impl<T: 'static> Computable for VarInner<T> {
    fn invalidate(&mut self) {
        self.value = None;
        let self_ptr = self.as_ptr();

        // Remove us from all dependencies
        // TODO: keep the memory here.
        for mut dependency in self.dependencies.drain() {
            unsafe { dependency.as_mut() }.remove_reader(self_ptr);
        }

        // Invalidate all readers (this will call remove_reader on self, so don't touch
        // `self.readers` while iterating)
        let mut readers = mem::take(&mut self.readers);
        for reader in &readers {
            unsafe { reader.clone().as_mut() }.invalidate();
        }
        readers.clear();
        // Readers are not allowed to be changed while invalidation runs.
        debug_assert!(self.readers.is_empty());
        // Put the empty readers back, to keep the allocated capacity for this var.
        self.readers = readers;
    }

    fn record_dependency(&mut self, dependency: ComputablePtr) {
        self.dependencies.insert(dependency);
    }

    fn remove_reader(&mut self, reader: ComputablePtr) {
        self.readers.remove(&reader);
    }
}

impl<T> Drop for VarInner<T> {
    fn drop(&mut self) {
        debug_assert!(self.readers.is_empty());
        let self_ptr = self.as_ptr();
        for dependency in &self.dependencies {
            unsafe { dependency.clone().as_mut() }.remove_reader(self_ptr);
        }
    }
}

/// This holds a pointer to a computable by preserving identity (note that trait objects can't be
/// compared reliably)
#[repr(transparent)]
#[derive(Clone, Copy, Eq)]
struct ComputablePtr(ptr::NonNull<dyn Computable>);

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
    fn new(computable: &dyn Computable) -> Self {
        ComputablePtr(unsafe {
            ptr::NonNull::new_unchecked(computable as *const dyn Computable as *mut dyn Computable)
        })
    }

    unsafe fn as_mut(&mut self) -> &mut dyn Computable {
        self.0.as_mut()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_if_hashset_keeps_its_capacity_after_clear() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(1);
        set.insert(2);
        set.insert(3);
        set.clear();
        assert!(set.capacity() >= 3);
    }
}
