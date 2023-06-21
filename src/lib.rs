mod runtime;
mod value;

pub use runtime::Runtime;
pub use value::Value;

#[macro_export]
macro_rules! computed {
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
macro_rules! computed_ref {
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

#[cfg(test)]
mod tests {
    use crate::runtime::Runtime;
    use std::{cell::RefCell, rc::Rc};

    #[test]
    fn add_two_vars() {
        let rt = Runtime::new();
        let a = rt.var(1);
        let mut b = rt.var(2);

        let c = computed!(|a, b| a + b);
        assert_eq!(c.get(), 3);
        b.set(3);
        assert_eq!(c.get(), 4);
    }

    #[test]
    fn diamond_problem() {
        let rt = Runtime::new();
        let mut a = rt.var(1);
        let b = computed!(|a| a * 2);
        let c = computed!(|a| a * 3);
        let evaluation_count = Rc::new(RefCell::new(0));
        let d = {
            let ec = evaluation_count.clone();
            computed!(|b, c| {
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
        let b = computed!(|a| a * 2);
        // b is not evaluated yet, so no readers.
        assert_eq!(a.readers_count(), 0);
        // Now we evaluate b, so it has a reader.
        assert_eq!(b.get(), 2);
        assert_eq!(a.readers_count(), 1);
        // Now we drop b, so it should remove its reader.
        drop(b);
        assert_eq!(a.readers_count(), 0);
    }

    #[test]
    fn changed_but_subsequently_subsequently_ignored_dependency_is_not_validated() {
        let rt = Runtime::new();
        let mut a = rt.var("a");
        let ac = computed!(|a| a);
        let mut switch = rt.var(false);
        let b = rt.var("b");
        let r = {
            let ac = ac.clone();
            let b = b.clone();
            computed!(|switch| if !switch { ac.get() } else { b.get() })
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
            computed!(|a| {
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
    fn simple_macro() {
        let rt = Runtime::new();
        let a = rt.var(1);
        let b = rt.var(2);
        let r = {
            let rt = rt.clone();
            computed!(|a, b| {
                let _just_here_to_see_if_clone_to_computed_works = rt.var(1);
                a + b
            })
        };
        assert_eq!(r.get(), 3);
        let c = rt.var(3);
        let r = computed!(|a, b, c| a + b + c);
        assert_eq!(r.get(), 6);
    }
}
