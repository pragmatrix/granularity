mod computed;
mod engine;
mod var;

pub use computed::Computed;
pub use engine::Engine;
pub use var::Var;

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use crate::engine::Engine;

    #[test]
    fn add_two_vars() {
        let engine = Engine::new();
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

    /// This wraps the `a` variable in a `RefCell` and drops it in the computation, even though it
    /// was read from and recorded as a dependency.
    #[test]
    fn pathological() {
        let engine = Engine::new();
        let a = engine.var(1);
        let b = engine.var(2);

        let c = {
            let b = b.share();
            let a = RefCell::new(a);
            let e = engine.clone();
            engine.computed(move || {
                let r = *a.borrow().get() + *b.get();
                // force a drop of a, even though it has readers.
                *a.borrow_mut() = e.var(1);
                r
            })
        };
        assert_eq!(*c.get(), 3);
    }
}
