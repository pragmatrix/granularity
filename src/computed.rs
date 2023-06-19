use crate::runtime::{self, Computable, ComputablePtr, Runtime};
use std::{
    cell::{Ref, RefCell},
    collections::HashSet,
    rc::Rc,
};

/// A computed value.
#[derive(Clone)]
pub struct Computed<'a, T: 'static>(Rc<RefCell<ComputedInner<'a, T>>>);

impl<'a, T> Computed<'a, T> {
    pub(crate) fn new(engine: &Rc<Runtime>, compute: impl FnMut() -> T + 'a) -> Self {
        let inner = ComputedInner {
            engine: engine.clone(),
            value: None,
            compute: Box::new(compute),
            readers: HashSet::new(),
            dependencies: HashSet::new(),
        };

        Computed(Rc::new(RefCell::new(inner)))
    }
}

impl<'a, T: 'static> Computed<'a, T> {
    pub fn get(&self) -> Ref<T> {
        {
            self.0.borrow_mut().ensure_valid();
        }

        // Add the current reader.
        {
            let reader = self.0.borrow().engine.current();
            if let Some(mut reader) = reader {
                let mut inner = self.0.borrow_mut();
                inner.readers.insert(reader);

                let reader = unsafe { reader.as_mut() };
                reader.record_dependency(inner.as_ptr());
            }
        }

        let r = self.0.borrow();
        Ref::map(r, |r| r.value.as_ref().unwrap())
    }
}

struct ComputedInner<'a, T: 'static> {
    engine: Rc<Runtime>,
    value: Option<T>,
    compute: Box<dyn FnMut() -> T + 'a>,
    // Readers are cleared when we invalidate.
    readers: HashSet<ComputablePtr>,
    // Deps are cleared on invalidation, too.
    dependencies: HashSet<ComputablePtr>,
}

impl<'a, T: 'static> ComputedInner<'a, T> {
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

impl<'a, T: 'static> Computable for ComputedInner<'a, T> {
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
        runtime::invalidate_readers(&mut self.readers);
    }

    fn record_dependency(&mut self, dependency: ComputablePtr) {
        self.dependencies.insert(dependency);
    }

    fn remove_reader(&mut self, reader: ComputablePtr) {
        self.readers.remove(&reader);
    }
}

impl<'a, T: 'static> Drop for ComputedInner<'a, T> {
    fn drop(&mut self) {
        debug_assert!(self.readers.is_empty());
        let self_ptr = self.as_ptr();
        for dependency in &self.dependencies {
            unsafe { dependency.clone().as_mut() }.remove_reader(self_ptr);
        }
    }
}
