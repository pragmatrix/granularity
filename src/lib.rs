mod computed;
mod engine;
mod var;

pub use computed::Computed;
pub use engine::Engine;
pub use var::Var;

#[cfg(test)]
mod tests {
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
}
