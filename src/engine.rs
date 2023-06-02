use std::cell::Ref;
use std::{cell::RefCell, collections::HashSet, ops::Deref, rc::Rc};
pub struct Engine {
    inner: RefCell<EngineInner>,
}

impl Engine {
    pub fn current(&self) -> *mut dyn Computable {
        self.inner.borrow().current
    }

    pub fn eval(&self, current: *mut dyn Computable, f: impl FnOnce()) {
        let mut inner = self.inner.borrow_mut();
        let prev = inner.current;
        inner.current = current;
        f();
        inner.current = prev;
    }
}

struct EngineInner {
    current: *mut dyn Computable,
}

/// A cheap to clone shared value.
#[derive(Clone)]
pub struct Var<T: 'static> {
    inner: Rc<RefCell<VarInner<T>>>,
}

impl<T> Var<T> {
    fn get(&self) -> Ref<T> {
        // Force before adding a reader, because we can't add readers on an invalid value.
        self.inner.borrow_mut().force();

        // Add the current reader.
        {
            let reader = self.inner.borrow().engine.inner.borrow().current;
            self.inner.borrow_mut().readers.insert(reader);

            let reader = unsafe { reader.as_mut().unwrap() };
            reader.record_dependency(self.inner.as_ptr() as *mut dyn Computable);
        }

        let r = self.inner.borrow();
        Ref::map(r, |r| r.value.as_ref().unwrap())
    }
}

struct VarInner<T: 'static> {
    engine: Rc<Engine>,
    value: Option<T>,
    compute: Box<dyn Fn() -> T>,
    // Readers are cleared when we invalidate.
    readers: HashSet<*mut dyn Computable>,
    // Deps are cleared on invalidation, too.
    dependencies: HashSet<*mut dyn Computable>,
}

impl<T: 'static> VarInner<T> {
    pub fn force(&mut self) -> &T {
        if self.value.is_none() {
            // Readers must be empty when recomputing.
            assert!(self.readers.is_empty());
            let self_ptr = self as *mut _;
            self.engine.eval(self_ptr, || {
                self.value = Some((self.compute)());
            });
        }
        self.value.as_ref().unwrap()
    }
}

trait Computable {
    fn invalidate(&mut self);
    fn force(&mut self);
    fn record_dependency(&mut self, dependency: *mut dyn Computable);
    fn remove_dependency(&mut self, dependency: *mut dyn Computable);
}

impl<T: 'static> Computable for VarInner<T> {
    fn force(&mut self) {
        self.force();
    }

    fn invalidate(&mut self) {
        self.value = None;
        // Invalidate all readers
        for reader in self.readers.drain() {
            unsafe { reader.as_mut().unwrap() }.invalidate();
        }
        let self_ptr = self as *mut _;
        // Remove us from all dependencies
        for dependency in self.dependencies.drain() {
            unsafe { dependency.as_mut().unwrap() }.remove_dependency(self_ptr);
        }
    }

    fn record_dependency(&mut self, dependency: *mut dyn Computable) {
        self.dependencies.insert(dependency);
    }

    fn remove_dependency(&mut self, dependency: *mut dyn Computable) {
        self.dependencies.remove(&dependency);
    }
}

impl<T> Drop for VarInner<T> {
    fn drop(&mut self) {
        self.invalidate()
    }
}
