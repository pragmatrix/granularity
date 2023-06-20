use crate::runtime::{
    self, Computable, ComputablePtr, RefCellComputable, RefCellComputableHandle, Runtime,
};
use std::{
    cell::{Ref, RefCell},
    collections::HashSet,
    rc::Rc,
};

/// A computed value.
#[derive(Clone)]
pub struct Computed<T: 'static>(Rc<RefCell<ComputedInner<T>>>);

impl<T> Computed<T> {
    pub(crate) fn new(runtime: &Rc<Runtime>, compute: impl FnMut() -> T + 'static) -> Self {
        let inner = ComputedInner {
            runtime: runtime.clone(),
            value: None,
            compute: Box::new(compute),
            readers: HashSet::new(),
            dependencies: HashSet::new(),
        };

        Computed(Rc::new(RefCell::new(inner)))
    }
}

impl<T: 'static> Computed<T> {
    pub fn get(&self) -> Ref<T> {
        {
            self.0.borrow_mut().ensure_valid();
        }

        // Add the current reader.
        {
            let reader = self.0.borrow().runtime.current();
            if let Some(mut reader) = reader {
                let mut inner = self.0.borrow_mut();
                inner.readers.insert(reader);

                let reader = unsafe { reader.as_mut() };
                reader.record_dependency(self.0.clone());
            }
        }

        let r = self.0.borrow();
        Ref::map(r, |r| r.value.as_ref().unwrap())
    }
}

struct ComputedInner<T: 'static> {
    runtime: Rc<Runtime>,
    value: Option<T>,
    compute: Box<dyn FnMut() -> T + 'static>,
    // Readers are cleared when we invalidate.
    readers: HashSet<ComputablePtr>,
    // Deps are cleared on invalidation, too.
    dependencies: HashSet<RefCellComputableHandle>,
}

impl<T: 'static> ComputedInner<T> {
    pub fn ensure_valid(&mut self) {
        if self.value.is_none() {
            // Readers must be empty when recomputing.
            assert!(self.readers.is_empty());
            let self_ptr = self.as_ptr();
            self.runtime.eval(self_ptr, || {
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

        // Remove us from all dependencies Because we may already be called from a dependency, we
        // can't use borrow_mut here.
        //
        // This is most likely even unsound, because we access two `&mut` references to the same
        // trait object.
        {
            for dependency in &self.dependencies {
                unsafe { dependency.as_mut().remove_reader(self_ptr) };
            }
            self.dependencies.clear();
        }

        // Invalidate all readers
        runtime::invalidate_readers(&mut self.readers);
    }

    fn record_dependency(&mut self, dependency: Rc<dyn RefCellComputable>) {
        self.dependencies
            .insert(RefCellComputableHandle(dependency));
    }

    fn remove_reader(&mut self, reader: ComputablePtr) {
        self.readers.remove(&reader);
    }
}

impl<T: 'static> Drop for ComputedInner<T> {
    fn drop(&mut self) {
        debug_assert!(self.readers.is_empty());
        let self_ptr = self.as_ptr();
        for dependency in &self.dependencies {
            dependency.borrow_mut().remove_reader(self_ptr);
        }
    }
}
