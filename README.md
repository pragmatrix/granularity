This experiment is a shallow dive into incremental computation. It implements a very basic, though somewhat usable automatic dependency tracking, invalidation, and recomputation system. You can take a look `lib.rs`'s testcases' for a very basic example on how to use it.

This implementation uses a pull / on-demand based "naive" approach for the simple reason that push-based approaches seem to be [a lot more complicated](https://www.janestreet.com/tech-talks/seven-implementations-of-incremental/) and even [semantically incorrect](https://github.com/salsa-rs/salsa/issues/41#issuecomment-589412839).

But a "naive" on-demand approach like in this repository suffers from too much unnecessary re-computation and therefore needs memoization to be efficient, which would be the next step to extend this library.

If you are interested in more information about self-adjusting and incremental computation:

Papers, Blog Posts, Talks:

- [Umut A. Acar - Self-Adjusting Computation](https://www.umut-acar.org/self-adjusting-computation)
- [How to Recalculate a Spreadsheet – Lord.io](https://lord.io/spreadsheets/)
- [Seven Implementations of Incremental :: Jane Street](https://www.janestreet.com/tech-talks/seven-implementations-of-incremental/)

Implementations (mostly Rust):

- [janestreet/incremental: A library for incremental computations](https://github.com/janestreet/incremental)
- [lord/anchors: self adjusting computations in rust](https://github.com/lord/anchors)
- [Adapton: Programming Language Abstractions for Incremental Computation](http://adapton.org/)
- [salsa-rs/salsa: A generic framework for on-demand, incrementalized computation. Inspired by adapton, glimmer, and rustc's query system.](https://github.com/salsa-rs/salsa)

