use crate::{
    computed::Computed,
    runtime::{self, Computable, ComputablePtr, RefCellComputable, Runtime},
};
use std::{
    cell::{Ref, RefCell},
    collections::HashSet,
    rc::Rc,
};

/// Vars need to be cheaply cloned, so that we can pass and share them easily.
///
/// The `RefCell` here protects from external access conflicts, but not from internal ones where
/// `&mut Computable` is used.
#[derive(Clone)]
pub struct Var<T: 'static>(Rc<RefCell<VarInner<T>>>);

impl<T> Var<T> {
    pub(crate) fn new(runtime: &Rc<Runtime>, value: T) -> Self {
        let inner = VarInner {
            runtime: runtime.clone(),
            value,
            readers: HashSet::new(),
        };
        Var(Rc::new(RefCell::new(inner)))
    }

    pub fn set(&mut self, value: T) {
        let mut inner = self.0.borrow_mut();
        inner.invalidate();
        inner.value = value;
    }

    pub fn computed<R>(self, mut f: impl FnMut(T) -> R + 'static) -> Computed<R>
    where
        T: Clone,
    {
        self.computed_ref(move |value| f(value.clone()))
    }

    pub fn computed_ref<R>(self, mut f: impl FnMut(&T) -> R + 'static) -> Computed<R> {
        let rt = self.0.borrow().runtime.clone();
        rt.computed(move || {
            let value = self.get_ref();
            f(&value)
        })
    }

    pub fn to_computed(&self) -> Computed<T>
    where
        T: Clone,
    {
        let cloned = self.clone();
        let rt = cloned.0.borrow().runtime.clone();
        Computed::new(&rt, move || cloned.get())
    }

    pub fn runtime(&self) -> Rc<Runtime> {
        self.0.borrow().runtime.clone()
    }

    #[cfg(test)]
    pub(crate) fn readers_count(&self) -> usize {
        self.0.borrow().readers.len()
    }
}

impl<T> Var<T> {
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.get_ref().clone()
    }

    pub fn get_ref(&self) -> Ref<T> {
        self.track();
        let r = self.0.borrow();
        Ref::map(r, |r| &r.value)
    }

    fn track(&self) {
        // Hold inner exclusively to blow on recursion.
        let mut inner = self.0.borrow_mut();
        let reader = inner.runtime.current();
        if let Some(mut reader) = reader {
            inner.readers.insert(reader);
            let reader = unsafe { reader.as_mut() };
            reader.record_dependency(self.0.clone());
        }
    }
}

struct VarInner<T: 'static> {
    runtime: Rc<Runtime>,
    value: T,
    readers: runtime::Readers,
}

impl<T: 'static> Computable for VarInner<T> {
    fn invalidate(&mut self) {
        runtime::invalidate_readers(&mut self.readers)
    }

    fn record_dependency(&mut self, _dependency: Rc<dyn RefCellComputable>) {
        panic!("Can't record dependencies on a var");
    }

    fn remove_reader(&mut self, reader: ComputablePtr) {
        self.readers.remove(&reader);
    }
}

impl<T> Drop for VarInner<T> {
    fn drop(&mut self) {
        debug_assert!(self.readers.is_empty());
    }
}
