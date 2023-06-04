use crate::{
    computed::Computed,
    engine::{self, Computable, ComputablePtr, Engine, AsPtr},
};
use std::{
    cell::{Ref, RefCell},
    collections::HashSet,
    rc::Rc,
};

pub struct Var<T: 'static> {
    // Vars need to be cheaply cloned, so that we can pass and share them easily.
    //
    // The `RefCell` here protects from external access conflicts, but not from internal ones where
    // `&mut Computable` is used.
    inner: Rc<RefCell<VarInner<T>>>,
}

impl<T> Var<T> {
    pub(crate) fn new(engine: &Rc<Engine>, value: T) -> Self {
        let inner = VarInner {
            engine: engine.clone(),
            value,
            readers: HashSet::new(),
        };
        Var {
            inner: Rc::new(RefCell::new(inner)),
        }
    }

    pub fn set(&mut self, value: T) {
        let mut inner = self.inner.borrow_mut();
        inner.invalidate();
        inner.value = value;
    }

    /// Share this as a computed value.
    pub fn share(&self) -> Computed<T>
    where
        T: Clone,
    {
        let cloned = self.clone();
        let engine = cloned.inner.borrow().engine.clone();
        Computed::new(&engine, move || cloned.get().clone())
    }

    fn clone(&self) -> Var<T> {
        Var {
            inner: self.inner.clone(),
        }
    }
}

impl<T> Var<T> {
    pub fn get(&self) -> Ref<T> {
        // Add the current reader.
        {
            // Hold inner exclusively to blow on recursion.
            let mut inner = self.inner.borrow_mut();
            let reader = inner.engine.current();
            if let Some(mut reader) = reader {
                inner.readers.insert(reader);
                let reader = unsafe { reader.as_mut() };
                reader.record_dependency(inner.as_ptr());
            }
        }

        let r = self.inner.borrow();
        Ref::map(r, |r| &r.value)
    }
}

struct VarInner<T: 'static> {
    engine: Rc<Engine>,
    value: T,
    readers: engine::Readers,
}

impl<T: 'static> Computable for VarInner<T> {
    fn invalidate(&mut self) {
        engine::invalidate_readers(&mut self.readers)
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
