use crate::{
    runtime::{self, Node, NodePtr, RefCellNode, RefCellNodeHandle, Runtime},
    versioning::ValueVersion,
};
use std::{
    cell::{Ref, RefCell},
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
    pub(crate) fn new_var(runtime: &Runtime, value: T) -> Self {
        let inner = ValueInner {
            runtime: runtime.clone(),
            version: runtime.new_var_version(),
            primitive: Var(value),
        };
        Value(Rc::new(RefCell::new(inner)))
    }

    pub(crate) fn new_computed(runtime: &Runtime, compute: impl FnMut() -> T + 'static) -> Self {
        let inner = ValueInner {
            runtime: runtime.clone(),
            version: runtime.new_computed_version(),
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
        self.ensure_valid_and_track_read();
        let r = self.0.borrow();
        Ref::map(r, |r| r.primitive.value().unwrap())
    }

    /// Track the value for receiving change notifications when it changes.
    pub fn track(&self) {
        self.ensure_valid_and_track_read();
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

    pub fn runtime(&self) -> Runtime {
        self.0.borrow().runtime.clone()
    }

    fn ensure_valid_and_track_read(&self) {
        let inner = self.0.try_borrow_mut();
        let Ok(mut inner) = inner else {
            // `inner` is already borrowed, this means that there are another `get_ref()` is active,
            // or there is a cycle in the evaluation. The former is fine if the value is valid.
            let inner = self.0.borrow();
            #[cfg(debug_assertions)]
            debug_assert!(inner.is_valid());
            self.track_read(&inner);
            return;
        };
        inner.ensure_valid();
        self.track_read(&inner);
    }

    fn track_read(&self, inner: &ValueInner<T>) {
        let reader = inner.runtime.current();
        if let Some(mut reader) = reader {
            let reader = unsafe { reader.as_mut() };
            reader.track_read_from(self.0.clone());
        }
    }

    #[cfg(test)]
    pub fn is_valid(&self) -> bool {
        self.0.borrow().is_valid()
    }
}

struct ValueInner<T: 'static> {
    runtime: Runtime,
    version: ValueVersion,
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
        let validated_version = self.runtime.validated_version();
        if self.version.validated == validated_version {
            return;
        }

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
                self.runtime.eval(self_ptr, || {
                    *value = Some(compute());
                });
            }
        }
        self.version.validated = validated_version;
    }

    #[cfg(debug_assertions)]
    /// Returns true if the value can be made valid without recomputing it.
    fn is_valid(&self) -> bool {
        match self.primitive {
            Var(_) => true,
            Computed { ref value, .. } => {
                value.is_some() && self.version.validated == self.runtime.validated_version()
            }
        }
    }

    fn as_ptr(&self) -> NodePtr {
        NodePtr::new(self)
    }
}

impl<T> Node for ValueInner<T> {
    fn invalidate(&mut self) {
        // Explicit invalidation is not transitive, but it drops the value.
        self.version.changed = self.runtime.change_version();

        // Clean up the value.
        {
            match self.primitive {
                Var(_) => {
                    // Vars are never dropped dropped (yet)
                }
                Computed {
                    ref mut value,
                    ref mut trace,
                    ..
                } => {
                    *value = None;
                    // Drop the trace and remove us from all dependencies
                    drop_trace(trace)
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
}

impl<T> Drop for ValueInner<T> {
    fn drop(&mut self) {
        match self.primitive {
            Var(_) => {}
            Computed { ref mut trace, .. } => {
                drop_trace(trace);
            }
        }
    }
}

/// Removes the trace and removes this node from all dependencies.
fn drop_trace(trace: &mut runtime::Trace) {
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
