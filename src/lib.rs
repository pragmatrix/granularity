mod runtime;
mod value;

pub use runtime::Runtime;
pub use value::Value;

#[macro_export]
macro_rules! map {
    (| $first:ident | $body:expr) => {{
        let $first = $first.clone();
        $first.runtime().computed(move || {
            let $first = $first.get();
            $body
        })
    }};

    (| $first:ident, $($rest:ident),* | $body:expr) => {{
        // Not so sure if we actually should clone here in any case. Also this prevents us from
        // passing expressions, which is probably is a good thing? IDK.
        let $first = $first.clone();
        $(let $rest = $rest.clone();)*
        $first.runtime().computed(move || {
            let $first = $first.get();
            $(let $rest = $rest.get();)*
            $body
        })
    }};
}

#[macro_export]
macro_rules! map_ref {
    (| $first:ident | $body:expr) => {{
        let $first = $first.clone();
        $first.runtime().computed(move || {
            let $first = &*$first.get_ref();
            $body
        })
    }};

    (| $first:ident, $($rest:ident),* | $body:expr) => {{
        // Not so sure if we actually should clone here in any case. Also this prevents us from
        // passing expressions, which is probably is a good thing? IDK.
        let $first = $first.clone();
        $(let $rest = $rest.clone();)*
        $first.runtime().computed(move || {
            let $first = &*$first.get_ref();
            $(let $rest = &*$rest.get_ref();)*
            $body
        })
    }};
}

#[macro_export]
macro_rules! memo {
    (| $first:ident | $body:expr) => {{
        let $first = $first.clone();
        $first.runtime().memo(
            move || $first.get(),
            move |$first| { $body }
        )
    }};

    (| $first:ident, $($rest:ident),* | $body:expr) => {{
        let $first = $first.clone();
        $(let $rest = $rest.clone();)*
        $first.runtime().memo(
            move || ($first.get(), $($rest.get()),*),
            move |($first, $($rest),*)| { $body }
        )

    }}
}

#[cfg(test)]
mod tests {
    use crate::runtime::Runtime;
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    #[test]
    fn add_two_vars() {
        let rt = Runtime::new();
        let a = rt.var(1);
        let mut b = rt.var(2);

        let c = map!(|a, b| a + b);
        assert_eq!(c.get(), 3);
        b.set(3);
        assert_eq!(c.get(), 4);
    }

    #[test]
    fn diamond_problem() {
        let rt = Runtime::new();
        let mut a = rt.var(1);
        let b = map!(|a| a * 2);
        let c = map!(|a| a * 3);
        let evaluation_count = Rc::new(RefCell::new(0));
        let d = {
            let ec = evaluation_count.clone();
            map!(|b, c| {
                *ec.borrow_mut() += 1;
                b + c
            })
        };
        assert_eq!(d.get(), 5);
        assert_eq!(*evaluation_count.borrow(), 1);

        a.set(2);
        assert_eq!(d.get(), 10);
        assert_eq!(*evaluation_count.borrow(), 2);
    }

    #[test]
    fn readers_are_removed_when_computed_is_dropped() {
        let rt = Runtime::new();
        let a = rt.var(1);
        let b = map!(|a| a * 2);
        // b is not evaluated yet, so no readers.
        assert_eq!(a.readers_count(), 0);
        // Now we evaluate b, so it has a reader.
        assert_eq!(b.get(), 2);
        assert_eq!(a.readers_count(), 1);
        // Now we drop b, so it should remove its reader.
        drop(b);
        assert_eq!(a.readers_count(), 0);
    }

    /// Test support support of the "switching pattern".
    /// See adapton: https://docs.rs/adapton/latest/adapton/#demand-driven-change-propagation
    #[test]
    fn div_check() {
        let rt = Runtime::new();

        // Two mutable inputs, for numerator and denominator of division
        let num = rt.var(42);
        let mut den = rt.var(2);

        // Two sub computations: The division, and a check thunk with a conditional expression
        let div = map!(|num, den| num / den);
        let check = map!(|den| if den == 0 { None } else { Some(div.get()) });

        // Observe output of `check` while we change the input `den`
        assert_eq!(check.get(), Some(21));

        den.set(0);
        assert_eq!(check.get(), None);

        den.set(2);
        assert_eq!(check.get(), Some(21)); // division is used again
    }

    /// Test for the "switching pattern" by checking `is_valid()`.
    #[test]
    fn changed_but_subsequently_subsequently_ignored_dependency_is_not_validated() {
        let rt = Runtime::new();
        let mut a = rt.var("a");
        let ac = map!(|a| a);
        let mut switch = rt.var(false);
        let b = rt.var("b");
        let r = {
            let ac = ac.clone();
            let b = b.clone();
            map!(|switch| if !switch { ac.get() } else { b.get() })
        };

        assert_eq!(r.get(), "a");

        {
            a.set("aa");
            switch.set(true);
        }

        assert_eq!(r.get(), "b");
        assert!(!ac.is_valid());
    }

    /// Drop `a` in a computation after it was read.
    #[test]
    fn drop_var_after_read_in_computed() {
        let rt = Runtime::new();
        let a = rt.var(1);
        let c = {
            let mut a = Some(a);
            rt.computed(move || {
                // read from a.
                let r = a.as_ref().unwrap().get();
                // Drop a, even though it has readers.
                a = None;
                r
            })
        };
        assert_eq!(c.get(), 1);
    }

    #[test]
    fn recorded_reader_gets_dropped() {
        let rt = Runtime::new();
        let a = rt.var(1);

        let drop_counter = Rc::new(());

        let b = {
            let r = drop_counter.clone();
            map!(|a| {
                let _b = a;
                r.clone()
            })
        };

        b.get();
        assert_eq!(Rc::strong_count(&drop_counter), 3);
        assert!(b.is_valid());
        assert_eq!(a.readers_count(), 1);

        drop(b);
        assert_eq!(a.readers_count(), 0);
        assert_eq!(Rc::strong_count(&drop_counter), 1);
    }

    #[test]
    fn simple_computed_macro() {
        let rt = Runtime::new();
        let a = rt.var(1);
        let b = rt.var(2);
        let r = {
            let rt = rt.clone();
            map!(|a, b| {
                let _just_here_to_see_if_clone_to_computed_works = rt.var(1);
                a + b
            })
        };
        assert_eq!(r.get(), 3);
        let c = rt.var(3);
        let r = map!(|a, b, c| a + b + c);
        assert_eq!(r.get(), 6);
    }

    #[test]
    fn simple_memo_macro() {
        let rt = Runtime::new();
        let mut a = rt.var(1);
        let count = Rc::new(Cell::new(0));
        let c = {
            let count = count.clone();
            map!(|a| {
                count.set(count.get() + 1);
                a + 1
            })
        };

        assert_eq!(c.get(), 2);
        assert_eq!(count.get(), 1);
        // be sure the invalidation gets through (set might later check for equality)
        a.set(2);
        a.set(1);
        assert_eq!(c.get(), 2);
        assert_eq!(count.get(), 2);

        count.set(0);

        let c = {
            let count = count.clone();
            memo!(|a| {
                count.set(count.get() + 1);
                a + 1
            })
        };

        assert_eq!(c.get(), 2);
        assert_eq!(count.get(), 1);
        // be sure the invalidation gets through (set might later check for equality)
        a.set(2);
        a.set(1);
        assert_eq!(c.get(), 2);
        assert_eq!(count.get(), 1);
    }
}
