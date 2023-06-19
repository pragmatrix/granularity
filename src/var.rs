use crate::{
    computed::Computed,
    runtime::{self, Computable, ComputablePtr, Runtime},
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
pub struct Var<T: 'static>(Rc<RefCell<VarInner<T>>>);

impl<T> Var<T> {
    pub(crate) fn new(rt: &Rc<Runtime>, value: T) -> Self {
        let inner = VarInner {
            rt: rt.clone(),
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

    /// Share this as a computed value.
    pub fn share(&self) -> Computed<'static, T>
    where
        T: Clone,
    {
        let cloned = self.clone();
        let rt = cloned.0.borrow().rt.clone();
        Computed::new(&rt, move || cloned.get().clone())
    }

    fn clone(&self) -> Var<T> {
        Var(self.0.clone())
    }

    #[cfg(test)]
    pub(crate) fn readers_count(&self) -> usize {
        self.0.borrow().readers.len()
    }
}

impl<T> Var<T> {
    pub fn get(&self) -> Ref<T> {
        // Add the current reader.
        {
            // Hold inner exclusively to blow on recursion.
            let mut inner = self.0.borrow_mut();
            let reader = inner.rt.current();
            if let Some(mut reader) = reader {
                inner.readers.insert(reader);
                let reader = unsafe { reader.as_mut() };
                reader.record_dependency(inner.as_ptr());
            }
        }

        let r = self.0.borrow();
        Ref::map(r, |r| &r.value)
    }
}

struct VarInner<T: 'static> {
    rt: Rc<Runtime>,
    value: T,
    readers: runtime::Readers,
}

impl<T: 'static> VarInner<T> {
    fn as_ptr(&self) -> ComputablePtr {
        ComputablePtr::new(self)
    }
}

impl<T: 'static> Computable for VarInner<T> {
    fn invalidate(&mut self) {
        runtime::invalidate_readers(&mut self.readers)
    }

    fn record_dependency(&mut self, _dependency: ComputablePtr) {
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
