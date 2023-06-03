use crate::engine::{Computable, ComputablePtr, Engine};
use std::{
    cell::{Cell, Ref, RefCell},
    collections::HashSet,
    hash, mem,
    ops::DerefMut,
    ptr,
    rc::Rc,
};

/// A cheap to clone variable.
#[derive(Clone)]
pub struct Var<T: 'static> {
    // Vars need to be cheaply cloned, so that we can pass and share them easily.
    //
    // The `RefCell` here protects from external access conflicts, but not from internal ones where
    // `&mut Computable` is used.
    inner: Rc<RefCell<VarInner<T>>>,
}

impl<T> Var<T> {
    pub fn new(engine: &Rc<Engine>, value: T) -> Self
    where
        T: Clone,
    {
        let inner = VarInner {
            engine: engine.clone(),
            value: None,
            compute: Box::new(move || value.clone()),
            readers: HashSet::new(),
            dependencies: HashSet::new(),
        };
        Var {
            inner: Rc::new(RefCell::new(inner)),
        }
    }

    pub fn computed(engine: &Rc<Engine>, compute: impl Fn() -> T + 'static) -> Self {
        let inner = VarInner {
            engine: engine.clone(),
            value: None,
            compute: Box::new(compute),
            readers: HashSet::new(),
            dependencies: HashSet::new(),
        };

        Var {
            inner: Rc::new(RefCell::new(inner)),
        }
    }

    pub fn set(&mut self, value: T)
    where
        T: Clone,
    {
        let mut inner = self.inner.borrow_mut();
        inner.invalidate();
        inner.compute = Box::new(move || value.clone());
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
            let reader = self.inner.borrow().engine.current();
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

impl<T: 'static> Computable for VarInner<T> {
    fn invalidate(&mut self) {
        self.value = None;
        let self_ptr = self.as_ptr();

        // Remove us from all dependencies
        {
            for dependency in &self.dependencies {
                unsafe { dependency.clone().as_mut() }.remove_reader(self_ptr);
            }
            self.dependencies.clear();
        }

        // Invalidate all readers (this will call remove_reader on self, so don't touch
        // `self.readers` while iterating)
        {
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
