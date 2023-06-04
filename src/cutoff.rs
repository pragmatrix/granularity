//! Idea: Computed values don't destroy their stored values, but instead just mark them as invalid.
//! And then cutoff points can be introduced to the graph where recomputation stops when the result
//! is equal to the previous value.

