mod computed;
mod runtime;
mod var;

pub use computed::Computed;
pub use runtime::Runtime;
pub use var::Var;

#[cfg(test)]
mod tests {
    use crate::runtime::Runtime;
    use std::{cell::RefCell, rc::Rc};

    #[test]
    fn add_two_vars() {
        let rt = Runtime::new();
        let a = rt.var(1);
        let mut b = rt.var(2);

        let c = {
            let b = b.share();
            rt.computed(move || *a.get() + *b.get())
        };
        assert_eq!(*c.get(), 3);
        b.set(3);
        assert_eq!(*c.get(), 4);
    }

    #[test]
    fn diamond_problem() {
        let rt = Runtime::new();
        let mut a = rt.var(1);

        let b = {
            let a = a.share();
            rt.computed(move || *a.get() * 2)
        };
        let a2 = a.share();
        let c = rt.computed(move || *a2.get() * 3);
        let evaluation_count = Rc::new(RefCell::new(0));
        let d = {
            let ec = evaluation_count.clone();
            rt.computed(move || {
                *ec.borrow_mut() += 1;
                *b.get() + *c.get()
            })
        };
        assert_eq!(*d.get(), 5);
        assert_eq!(*evaluation_count.borrow(), 1);

        a.set(2);
        assert_eq!(*d.get(), 10);
        assert_eq!(*evaluation_count.borrow(), 2);
    }

    #[test]
    fn readers_are_removed_when_computed_is_dropped() {
        let rt = Runtime::new();
        let a = rt.var(1);
        let b = {
            let a = a.share();
            rt.computed(move || *a.get() * 2)
        };
        // b is not evaluated yet, so no readers.
        assert_eq!(a.readers_count(), 0);
        // Now we evaluate b, so it has a reader.
        assert_eq!(*b.get(), 2);
        assert_eq!(a.readers_count(), 1);
        // Now we drop b, so it should remove its reader.
        drop(b);
        assert_eq!(a.readers_count(), 0);
    }

    /// Drop `a` in a computation after it was read.
    #[test]
    fn drop_var_after_read_in_computed() {
        let rt = Runtime::new();
        let a = rt.var(1);
        let b = rt.var(2);

        let c = {
            let b = b.share();
            let mut a = Some(a);
            rt.computed(move || {
                let r = *a.as_ref().unwrap().get() + *b.get();
                // Drop a, even though it has readers.
                a = None;
                r
            })
        };
        assert_eq!(*c.get(), 3);
    }
}
