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
pub struct Value<T: 'static>(Rc<RefCell<ValueInner<T>>>);

impl<T> Value<T> {
    pub(crate) fn new_var(runtime: &Rc<Runtime>, value: T) -> Self {
        let inner = ValueInner {
            runtime: runtime.clone(),
            readers: HashSet::new(),
            primitive: Primitive::Var(value),
        };
        Value(Rc::new(RefCell::new(inner)))
    }

    pub(crate) fn new_computed(
        runtime: &Rc<Runtime>,
        compute: impl FnMut() -> T + 'static,
    ) -> Self {
        let inner = ValueInner {
            runtime: runtime.clone(),
            readers: HashSet::new(),
            primitive: Primitive::Computed(Computed {
                value: None,
                compute: Box::new(compute),
                dependencies: Vec::new(),
            }),
        };

        Value(Rc::new(RefCell::new(inner)))
    }

    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.get_ref().clone()
    }

    pub fn get_ref(&self) -> Ref<T> {
        self.ensure_valid_and_track();
        let r = self.0.borrow();
        Ref::map(r, |r| r.primitive.value().unwrap())
    }

    pub fn set(&mut self, value: T) {
        let mut inner = self.0.borrow_mut();
        inner.set(value);
    }

    pub fn computed<R>(self, mut f: impl FnMut(T) -> R + 'static) -> Value<R>
    where
        T: Clone,
    {
        self.computed_ref(move |value| f(value.clone()))
    }

    pub fn computed_ref<R>(self, mut f: impl FnMut(&T) -> R + 'static) -> Value<R> {
        let rt = self.0.borrow().runtime.clone();
        rt.computed(move || {
            let value = self.get_ref();
            f(&value)
        })
    }

    pub fn runtime(&self) -> Rc<Runtime> {
        self.0.borrow().runtime.clone()
    }

    fn ensure_valid_and_track(&self) {
        let mut inner = self.0.borrow_mut();
        inner.ensure_valid();

        let reader = inner.runtime.current();
        if let Some(mut reader) = reader {
            inner.readers.insert(reader);

            let reader = unsafe { reader.as_mut() };
            reader.record_dependency(self.0.clone());
        }
    }

    #[cfg(test)]
    pub fn is_valid(&self) -> bool {
        self.0.borrow().primitive.value().is_some()
    }

    #[cfg(test)]
    pub(crate) fn readers_count(&self) -> usize {
        self.0.borrow().readers.len()
    }
}

struct ValueInner<T: 'static> {
    runtime: Rc<Runtime>,
    // Readers are cleared when we invalidate.
    readers: runtime::Readers,
    primitive: Primitive<T>,
}

struct Computed<T> {
    value: Option<T>,
    compute: Box<dyn FnMut() -> T>,
    // Dependencies that were tracked in the last evaluation.
    // Might contain duplicates.
    // Cleared on invalidation.
    dependencies: runtime::Dependencies,
}

enum Primitive<T> {
    Var(T),
    Computed(Computed<T>),
}

impl<T> Primitive<T> {
    fn value(&self) -> Option<&T> {
        match self {
            Primitive::Var(value) => Some(value),
            Primitive::Computed(computed) => computed.value.as_ref(),
        }
    }

    fn set(&mut self, value: T) {
        match self {
            Primitive::Var(ref mut var) => {
                *var = value;
            }
            Primitive::Computed(_) => {
                panic!("Cannot set a computed value")
            }
        }
    }
}

impl<T> ValueInner<T> {
    fn set(&mut self, value: T) {
        // TODO: only relevant in the Var path
        self.invalidate();
        self.primitive.set(value);
    }

    pub fn ensure_valid(&mut self) {
        // TODO: `self_ptr` is only used in the `Computed` path.
        let self_ptr = self.as_ptr();
        match self.primitive {
            Primitive::Var(_) => {
                // Always valid
            }
            Primitive::Computed(ref mut computed) => {
                if computed.value.is_none() {
                    // Readers must be empty when recomputing.
                    assert!(self.readers.is_empty());
                    self.runtime.eval(self_ptr, || {
                        computed.value = Some((computed.compute)());
                    });
                }
            }
        }
    }

    fn as_ptr(&self) -> ComputablePtr {
        ComputablePtr::new(self)
    }
}

impl<T> Computable for ValueInner<T> {
    fn invalidate(&mut self) {
        // TODO: `self_ptr` is only used in the `Computed` path.
        let self_ptr = self.as_ptr();
        match self.primitive {
            Primitive::Var(_) => {}
            Primitive::Computed(ref mut computed) => {
                computed.value = None;
                // Remove us from all dependencies Because we may already be called from a dependency, we
                // can't use borrow_mut here.
                //
                // This is most likely unsound, because we access two `&mut` references to the same trait
                // object.
                for dependency in &computed.dependencies {
                    unsafe { dependency.as_mut().remove_reader(self_ptr) };
                }
                computed.dependencies.clear();
            }
        }

        // Invalidate all readers
        runtime::invalidate_readers(&mut self.readers);
    }

    fn record_dependency(&mut self, dependency: Rc<dyn RefCellComputable>) {
        match self.primitive {
            Primitive::Var(_) => {
                panic!("A var does not support dependencies");
            }
            Primitive::Computed(ref mut computed) => computed
                .dependencies
                .push(RefCellComputableHandle(dependency)),
        }
    }

    fn remove_reader(&mut self, reader: ComputablePtr) {
        self.readers.remove(&reader);
    }
}

impl<T> Drop for ValueInner<T> {
    fn drop(&mut self) {
        debug_assert!(self.readers.is_empty());

        // TODO: `self_ptr` is only used in the `Computed` path.
        let self_ptr = self.as_ptr();

        match self.primitive {
            Primitive::Var(_) => {}
            Primitive::Computed(ref mut computed) => {
                // Remove us from all dependencies
                for dependency in &computed.dependencies {
                    dependency.borrow_mut().remove_reader(self_ptr);
                }
            }
        }
    }
}
