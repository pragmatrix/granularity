use crate::value::Value;
use std::{
    cell::{Cell, RefCell, RefMut},
    collections::HashSet,
    hash, ptr,
    rc::Rc,
};

#[derive(Clone)]
pub struct Runtime(Rc<RuntimeInner>);

impl Runtime {
    // "default Runtime" sounds something like a default runtime for the current context (like a
    // thread local one). So therefore no ::default() for now.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Runtime {
        Runtime(Rc::new(RuntimeInner::default()))
    }

    pub fn var<T>(&self, value: T) -> Value<T> {
        Value::new_var(self, value)
    }

    pub fn computed<T>(&self, compute: impl FnMut() -> T + 'static) -> Value<T> {
        Value::new_computed(self, compute)
    }

    /// Create a computed value that memoizes its result.
    ///
    /// The `key` function is invoked to determine if the value should be recomputed. If the key
    /// changes, the `compute` function is called. If the key is the same, the previous value is
    /// returned.
    ///
    /// `K` needs to implement `PartialEq` so that the key can be compared to the previous key and
    /// `T` to implement `Clone` so that the previous value can be returned.
    ///
    /// `T` should also be cheap to clone, e.g `Rc`, since it is stored two times in the node. In
    /// the cache, and as the result in the computed node.
    ///
    /// Even though tracked, dependencies that were invalidated and tracked _only_ in the compute
    /// function may not cause the value to be recomputed when the key stays the same. `compute`
    /// should therefore not resolve _any_ node values belonging to the same runtime. This might
    /// even be tested for in future updates.
    pub fn memo<K, T>(
        &self,
        key: impl Fn() -> K + 'static,
        mut compute: impl FnMut(&K) -> T + 'static,
    ) -> Value<T>
    where
        K: PartialEq + 'static,
        T: Clone,
    {
        let mut prev: Option<(K, T)> = None;
        Value::new_computed(self, move || {
            let key = key();
            if let Some((prev_key, prev_value)) = &prev {
                if key == *prev_key {
                    return prev_value.clone();
                }
            }
            let value = compute(&key);
            prev = Some((key, value.clone()));
            value
        })
    }

    pub(crate) fn eval(&self, current: NodePtr, f: impl FnOnce()) {
        let inner = &*self.0;
        let prev = inner.current.get();
        inner.current.set(Some(current));
        f();
        inner.current.set(prev);
    }

    pub(crate) fn current(&self) -> Option<NodePtr> {
        self.0.current.get()
    }
}

#[derive(Default)]
struct RuntimeInner {
    /// The currently evaluating value.
    current: Cell<Option<NodePtr>>,
}

pub trait Node {
    fn invalidate(&mut self);
    fn track_read_from(&mut self, from: Rc<dyn RefCellNode>);
    fn remove_reader(&mut self, reader: NodePtr);
}

pub trait RefCellNode {
    fn as_ptr(&self) -> NodePtr;

    fn borrow_mut(&self) -> RefMut<dyn Node>;

    #[allow(clippy::mut_from_ref)]
    unsafe fn as_mut(&self) -> &mut dyn Node;
}

impl<T> RefCellNode for RefCell<T>
where
    T: Node,
{
    fn as_ptr(&self) -> NodePtr {
        NodePtr::new(unsafe { &*RefCell::as_ptr(self) })
    }

    fn borrow_mut(&self) -> RefMut<dyn Node> {
        RefMut::map(self.borrow_mut(), |t| t as &mut dyn Node)
    }

    unsafe fn as_mut(&self) -> &mut dyn Node {
        (&mut *RefCell::as_ptr(self)) as &mut dyn Node
    }
}

#[derive(Clone)]
pub(crate) struct RefCellNodeHandle(pub Rc<dyn RefCellNode>);

impl PartialEq for RefCellNodeHandle {
    fn eq(&self, other: &Self) -> bool {
        // Can't compare trait ptrs using Rc::ptr_eq.
        let ptr_self = self.0.as_ptr();
        let ptr_other = other.0.as_ptr();
        ptr_self == ptr_other
    }
}

impl Eq for RefCellNodeHandle {}

impl hash::Hash for RefCellNodeHandle {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        Rc::as_ptr(&self.0).hash(state)
    }
}

impl RefCellNodeHandle {
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn as_mut(&self) -> &mut dyn Node {
        self.0.as_mut()
    }
}

/// This holds a pointer to a node by preserving identity (trait objects can't be compared equality
/// because their vtable pointer is not stable).
#[repr(transparent)]
#[derive(Clone, Copy, Eq)]
pub struct NodePtr(ptr::NonNull<dyn Node>);

impl PartialEq for NodePtr {
    fn eq(&self, other: &Self) -> bool {
        self.0.cast::<()>() == other.0.cast()
    }
}

impl hash::Hash for NodePtr {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.0.cast::<()>().hash(state)
    }
}

impl NodePtr {
    pub fn new(node: &dyn Node) -> Self {
        NodePtr(unsafe { ptr::NonNull::new_unchecked(node as *const dyn Node as *mut dyn Node) })
    }

    pub unsafe fn as_mut(&mut self) -> &mut dyn Node {
        self.0.as_mut()
    }
}

pub(crate) type Readers = HashSet<NodePtr>;
pub(crate) type Trace = Vec<RefCellNodeHandle>;

#[cfg(test)]
mod tests {
    #[test]
    fn hash_set_keeps_capacity_after_clear() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(1);
        set.insert(2);
        set.insert(3);
        set.clear();
        assert!(set.capacity() >= 3);
    }
}
