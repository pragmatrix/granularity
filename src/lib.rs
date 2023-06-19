mod computed;
mod runtime;
mod var;

pub use computed::Computed;
pub use runtime::Runtime;
pub use var::Var;

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use crate::runtime::Runtime;

    #[test]
    fn add_two_vars() {
        let engine = Runtime::new();
        let a = engine.var(1);
        let mut b = engine.var(2);

        let c = {
            let b = b.share();
            engine.computed(move || *a.get() + *b.get())
        };
        assert_eq!(*c.get(), 3);
        b.set(3);
        assert_eq!(*c.get(), 4);
    }

    #[test]
    fn diamond_problem() {
        let engine = Runtime::new();
        let mut a = engine.var(1);

        let b = {
            let a = a.share();
            engine.computed(move || *a.get() * 2)
        };
        let a2 = a.share();
        let c = engine.computed(move || *a2.get() * 3);
        let evaluation_count = RefCell::new(0);
        let d = {
            engine.computed(|| {
                *evaluation_count.borrow_mut() += 1;
                *b.get() + *c.get()
            })
        };
        assert_eq!(*d.get(), 5);
        assert_eq!(*evaluation_count.borrow(), 1);

        a.set(2);
        assert_eq!(*d.get(), 10);
        assert_eq!(*evaluation_count.borrow(), 2);
    }

    /// This wraps the `a` variable in a `RefCell` and drops it in the computation, even though it
    /// was read from and recorded as a dependency.
    #[test]
    fn pathological() {
        let rt = Runtime::new();
        let a = rt.var(1);
        let b = rt.var(2);

        let c = {
            let b = b.share();
            let a = RefCell::new(a);
            let rt2 = rt.clone();
            rt.computed(move || {
                let r = *a.borrow().get() + *b.get();
                // force a drop of a, even though it has readers.
                *a.borrow_mut() = rt2.var(1);
                r
            })
        };
        assert_eq!(*c.get(), 3);
    }
}
