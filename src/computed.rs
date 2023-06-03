use crate::engine::{self, Computable, ComputablePtr, Engine};
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
pub struct Computed<T: 'static> {
    inner: Rc<RefCell<ComputedInner<T>>>,
}

impl<T> Computed<T> {
    pub fn new(engine: &Rc<Engine>, compute: impl Fn() -> T + 'static) -> Self {
        let inner = ComputedInner {
            engine: engine.clone(),
            value: None,
            compute: Box::new(compute),
            readers: HashSet::new(),
            dependencies: HashSet::new(),
        };

        Computed {
            inner: Rc::new(RefCell::new(inner)),
        }
    }
}

impl<T> Computed<T> {
    pub fn get(&self) -> Ref<T> {
        {
            self.inner.borrow_mut().ensure_valid();
        }

        // Add the current reader.
        {
            let reader = self.inner.borrow().engine.current();
            if let Some(mut reader) = reader {
                let mut inner = self.inner.borrow_mut();
                inner.readers.insert(reader);

                let reader = unsafe { reader.as_mut() };
                reader.record_dependency(inner.as_ptr());
            }
        }

        let r = self.inner.borrow();
        Ref::map(r, |r| r.value.as_ref().unwrap())
    }
}

struct ComputedInner<T: 'static> {
    engine: Rc<Engine>,
    value: Option<T>,
    compute: Box<dyn Fn() -> T>,
    // Readers are cleared when we invalidate.
    readers: HashSet<ComputablePtr>,
    // Deps are cleared on invalidation, too. Rc is used to hold dependencies in memory.
    dependencies: HashSet<ComputablePtr>,
}

impl<T: 'static> ComputedInner<T> {
    pub fn ensure_valid(&mut self) {
        if self.value.is_none() {
            // Readers must be empty when recomputing.
            assert!(self.readers.is_empty());
            let self_ptr = self.as_ptr();
            self.engine.eval(self_ptr, || {
                self.value = Some((self.compute)());
            });
        }
    }

    fn as_ptr(&self) -> ComputablePtr {
        ComputablePtr::new(self)
    }
}

impl<T: 'static> Computable for ComputedInner<T> {
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

        // Invalidate all readers
        engine::invalidate_readers(&mut self.readers);
    }

    fn record_dependency(&mut self, dependency: ComputablePtr) {
        self.dependencies.insert(dependency);
    }

    fn remove_reader(&mut self, reader: ComputablePtr) {
        self.readers.remove(&reader);
    }
}

impl<T> Drop for ComputedInner<T> {
    fn drop(&mut self) {
        debug_assert!(self.readers.is_empty());
        let self_ptr = self.as_ptr();
        for dependency in &self.dependencies {
            unsafe { dependency.clone().as_mut() }.remove_reader(self_ptr);
        }
    }
}
