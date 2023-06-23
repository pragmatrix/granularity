use crate::runtime::{
    self, Computable, ComputablePtr, RefCellComputable, RefCellComputableHandle, Runtime,
};
use std::{
    cell::{Ref, RefCell},
    collections::HashSet,
    mem,
    rc::Rc,
};

/// This is a cheap to clone front end to a node in the dependency graph which represents either a
/// variable that is mutable or a computed value.
///
/// Create instances of this type using the `Runtime::var` and `Runtime::computed` methods.
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
            primitive: Primitive::Computed {
                value: None,
                compute: Box::new(compute),
                trace: Vec::new(),
            },
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
            reader.track_read_from(self.0.clone());
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
    // The nodes that read from this node. Nodes reading from this node are responsible for removing
    // themselves from us in their drop implementation.
    readers: runtime::Readers,
    primitive: Primitive<T>,
}

enum Primitive<T> {
    Var(T),
    Computed {
        value: Option<T>,
        compute: Box<dyn FnMut() -> T>,
        // Nodes that this node read from in the previous evaluation.
        // Might contain duplicates and locks them in memory via `Rc`.
        // Cleared on invalidation.
        trace: runtime::Trace,
    },
}

impl<T> Primitive<T> {
    fn value(&self) -> Option<&T> {
        match self {
            Primitive::Var(value) => Some(value),
            Primitive::Computed { value, .. } => value.as_ref(),
        }
    }

    fn set(&mut self, value: T) {
        match self {
            Primitive::Var(ref mut var) => {
                *var = value;
            }
            Primitive::Computed { .. } => {
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
            Primitive::Computed {
                ref mut value,
                ref mut compute,
                ..
            } => {
                if value.is_none() {
                    // Readers must be empty when recomputing.
                    assert!(self.readers.is_empty());
                    self.runtime.eval(self_ptr, || {
                        *value = Some(compute());
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
            Primitive::Computed {
                ref mut value,
                ref mut trace,
                ..
            } => {
                *value = None;
                // Remove us from all dependencies Because we may already be called from a dependency, we
                // can't use borrow_mut here.
                //
                // This is most likely unsound, because we access two `&mut` references to the same trait
                // object.
                drop_trace(self_ptr, trace)
            }
        }

        // Invalidate all readers
        {
            let mut readers = mem::take(&mut self.readers);
            for reader in &readers {
                unsafe { reader.clone().as_mut() }.invalidate();
            }
            readers.clear();
            // Readers are not allowed to be changed while invalidation runs.
            debug_assert!(self.readers.is_empty());
            // Put the empty readers back, to keep the allocated capacity.
            self.readers = readers;
        };
    }

    fn track_read_from(&mut self, from: Rc<dyn RefCellComputable>) {
        match self.primitive {
            Primitive::Var(_) => {
                panic!("A var does not support dependencies");
            }
            Primitive::Computed { ref mut trace, .. } => trace.push(RefCellComputableHandle(from)),
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
            Primitive::Computed {
                trace: ref mut dependencies,
                ..
            } => {
                drop_trace(self_ptr, dependencies);
            }
        }
    }
}

/// Removes the trace and removes this node from all dependencies.
fn drop_trace(self_ptr: ComputablePtr, trace: &mut runtime::Trace) {
    for dependency in trace.iter() {
        unsafe { dependency.as_mut().remove_reader(self_ptr) };
    }
    // TODO: when called from drop(), this is redundant.
    trace.clear();
}
