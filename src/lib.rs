mod engine;

#[cfg(test)]
mod tests {
    use crate::engine::Engine;

    #[test]
    fn add_two_vars() {
        let engine = Engine::new();
        let a = engine.var(1);
        let b = engine.var(2);

        let c = engine.computed(move || *a.get() + *b.get());
    }
}
