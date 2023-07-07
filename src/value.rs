use crate::runtime::{self, Node, NodePtr, RefCellNode, RefCellNodeHandle, Runtime};
use std::{
    cell::{Ref, RefCell},
    collections::HashSet,
    mem,
    rc::Rc,
};
use Primitive::*;

/// This is a cheap to clone front end to a node in the dependency graph which represents either a
/// variable that is mutable or a computed value.
///
/// Create instances of this type using the `Runtime::var` and `Runtime::computed` methods.
pub struct Value<T: 'static>(Rc<RefCell<ValueInner<T>>>);

impl<T> Clone for Value<T> {
    fn clone(&self) -> Self {
        Value(self.0.clone())
    }
}

impl<T> Value<T> {
    pub(crate) fn new_var(runtime: &Rc<Runtime>, value: T) -> Self {
        let inner = ValueInner {
            runtime: runtime.clone(),
            readers: HashSet::new(),
            primitive: Var(value),
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
            primitive: Computed {
                value: None,
                compute: Box::new(compute),
                trace: Vec::new(),
            },
        };

        Value(Rc::new(RefCell::new(inner)))
    }

    /// If needed, evaluates the value, then clones it and returns it. Requires the contained value to implement
    /// `Clone`.
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.get_ref().clone()
    }

    /// Evaluates the value and returns a reference to the contained value.
    pub fn get_ref(&self) -> Ref<T> {
        self.ensure_valid_and_track();
        let r = self.0.borrow();
        Ref::map(r, |r| r.primitive.value().unwrap())
    }

    /// Makes sure the value is evaluated then takes it out and invalidates it.
    ///
    /// This can't be called inside a evaluation context.
    pub fn take(&mut self) -> T {
        let mut inner = self.0.borrow_mut();
        debug_assert!(inner.runtime.current().is_none());
        inner.take()
    }

    pub fn set(&mut self, value: T) {
        self.apply(|_| value);
    }

    pub fn apply(&mut self, f: impl FnOnce(T) -> T) {
        self.0.borrow_mut().apply(f);
    }

    pub fn runtime(&self) -> Rc<Runtime> {
        self.0.borrow().runtime.clone()
    }

    fn ensure_valid_and_track(&self) {
        let inner = self.0.try_borrow_mut();
        let Ok(mut inner) = inner else {
            // `inner` is already borrowed, this means that there are another `get_ref()` is active,
            // or there is a cycle in the evaluation. The former is fine if the value is valid.
            let inner = self.0.borrow();
            debug_assert!(inner.is_valid());

            return;
        };
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
            Var(value) => Some(value),
            Computed { value, .. } => value.as_ref(),
        }
    }

    fn apply(&mut self, f: impl FnOnce(T) -> T) {
        match self {
            Var(ref mut var) => replace_with::replace_with_or_abort(var, f),
            Computed { .. } => {
                panic!("Cannot set a computed value")
            }
        }
    }
}

impl<T> ValueInner<T> {
    fn apply(&mut self, f: impl FnOnce(T) -> T) {
        // TODO: only relevant in the Var path
        self.invalidate();
        self.primitive.apply(f);
    }

    pub fn take(&mut self) -> T {
        self.ensure_valid();
        match self.primitive {
            Var(_) => panic!("Cannot take a var"),
            Computed { ref mut value, .. } => {
                // TODO: Consider returning the value from invalidate().
                let value = value.take().unwrap();
                self.invalidate();
                value
            }
        }
    }

    pub fn ensure_valid(&mut self) {
        // TODO: `self_ptr` is only used in the `Computed` path.
        let self_ptr = self.as_ptr();
        match self.primitive {
            Var(_) => {
                // Always valid
            }
            Computed {
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

    #[cfg(debug_assertions)]
    fn is_valid(&self) -> bool {
        match self.primitive {
            Var(_) => true,
            Computed { ref value, .. } => value.is_some(),
        }
    }

    fn as_ptr(&self) -> NodePtr {
        NodePtr::new(self)
    }
}

impl<T> Node for ValueInner<T> {
    fn invalidate(&mut self) {
        // We first invalidate all readers so that their values are dropped in reverse order. They
        // may indirectly depend on this value here to be alive and if not, it's nice to see that
        // dependencies are dropped first.
        //
        // TODO: Validate this behavior by creating a test case that checks the drop order.
        {
            let mut readers = mem::take(&mut self.readers);
            for reader in &readers {
                unsafe { reader.clone().as_mut() }.invalidate();
            }
            // Readers in this instance not allowed to change while invalidation runs.
            debug_assert!(self.readers.is_empty());
            // Clear the readers and put it back to keep the capacity.
            readers.clear();
            self.readers = readers;
        };

        // Clean up this value last
        {
            // TODO: `self_ptr` is only used in the `Computed` path.
            let self_ptr = self.as_ptr();
            match self.primitive {
                Var(_) => {}
                Computed {
                    ref mut value,
                    ref mut trace,
                    ..
                } => {
                    *value = None;
                    // Drop the trace and remove us from all dependencies Because we may already be
                    // called from a dependency, we can't use `borrow_mut` here.
                    //
                    // This is most likely unsound, because we access two `&mut` references to the same
                    // trait object.
                    drop_trace(self_ptr, trace)
                }
            }
        }
    }

    fn track_read_from(&mut self, from: Rc<dyn RefCellNode>) {
        match self.primitive {
            Var(_) => {
                panic!("A var does not support tracing dependencies");
            }
            Computed { ref mut trace, .. } => trace.push(RefCellNodeHandle(from)),
        }
    }

    fn remove_reader(&mut self, reader: NodePtr) {
        self.readers.remove(&reader);
    }
}

impl<T> Drop for ValueInner<T> {
    fn drop(&mut self) {
        debug_assert!(self.readers.is_empty());

        // TODO: `self_ptr` is only used in the `Computed` path.
        let self_ptr = self.as_ptr();

        match self.primitive {
            Var(_) => {}
            Computed { ref mut trace, .. } => {
                drop_trace(self_ptr, trace);
            }
        }
    }
}

/// Removes the trace and removes this node from all dependencies.
fn drop_trace(self_ptr: NodePtr, trace: &mut runtime::Trace) {
    for dependency in trace.iter() {
        unsafe { dependency.as_mut().remove_reader(self_ptr) };
    }
    // TODO: when called from drop(), this is redundant.
    trace.clear();
}

#[cfg(test)]
mod tests {
    use crate::Runtime;

    /// This is a syntax test. Values must support `clone()` even if their contained value is not.
    #[test]
    fn values_can_be_cloned() {
        let runtime = Runtime::new();
        struct Unique;
        let value = runtime.var(Unique);
        #[allow(clippy::redundant_clone)]
        let _ = value.clone();
    }
}
