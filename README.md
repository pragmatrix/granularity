This project implements an incomplete fine-grained reactivity graph. It provides rudimentary, though usable, reactive primitives, automatic dependency tracking, invalidation, and recomputation. 

`lib.rs`'s show cases basic examples on how to use it.

Granularity uses a pull / on-demand based "naive" approach because push-based approaches seem to be [a lot more complicated](https://www.janestreet.com/tech-talks/seven-implementations-of-incremental/) and even [semantically incorrect](https://github.com/salsa-rs/salsa/issues/41#issuecomment-589412839).

### Goal

The goal of this project is to provide a foundation for Granularity UI. A user interface framework that is built based on reactive primitives.

### Problems To Solve

#### Events vs. Signals

I've experimented with reactivity in user interfaces a long time ago. Around 2005 I've implemented a subset of a CSS3 layout engine based on hierarchical attribute trees I wrote in C#. And after building some application on my own, the need to use event sourcing for updating and persisting state change arose, which somehow seemed incompatible with reactive primitives. So I've put the idea into a box for a while. But now I think that - with a bit of discipline and a few helpers - these two concepts can be combined just fine.

#### Lifetime Management & Higher Order Primitives

Looking and Leptos and Sycamore, I've found that there is this need to introduce a kind of evaluation / lifetime scope that bounds the lifetime of the reactive primitives. I don't know if this is a requirement, but what I need from a reactive system are just the primitives without any context or lifetime bounds that is to care about. This is especially important when the primitives themselves need to be passed through the graph. For example, a layout engine - based on reactive primitives - may compute frame coordinates first while leaving the content primitives untouched and then passes them to the renderer which then recomputes them if needed.

#### Performance

For now not raw performance is not a primary concern. Granularity has slightly different requirements than web frameworks, so it's probably a lot slower in the beginning, but as soon there are a number of a test cases and perhaps even a project built on top of it, it will be easier to identify bottlenecks and optimize them.

Algorithmic performance is important though. Specifically the reuse of already computed values, aka memoization, needs to be solved properly and transparently to make Granularity usable.

### Inspiration

Here is some of the material I went through:

Papers, Blog Posts, Talks:

- [Umut A. Acar - Self-Adjusting Computation](https://www.umut-acar.org/self-adjusting-computation)
- [How to Recalculate a Spreadsheet – Lord.io](https://lord.io/spreadsheets/)
- [Seven Implementations of Incremental :: Jane Street](https://www.janestreet.com/tech-talks/seven-implementations-of-incremental/)

Implementations (mostly Rust):

- [janestreet/incremental: A library for incremental computations](https://github.com/janestreet/incremental)
- [lord/anchors: self adjusting computations in rust](https://github.com/lord/anchors)
- [Adapton: Programming Language Abstractions for Incremental Computation](http://adapton.org/)
- [salsa-rs/salsa: A generic framework for on-demand, incrementalized computation. Inspired by adapton, glimmer, and rustc's query system.](https://github.com/salsa-rs/salsa)

Rust Web Frameworks using fine-grained reactivity:

- [leptos-rs/leptos: Build fast web applications with Rust.](https://github.com/leptos-rs/leptos)
- [sycamore-rs/sycamore: A library for creating reactive web apps in Rust and WebAssembly](https://github.com/sycamore-rs/sycamore)

License: MIT
