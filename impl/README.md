# `for-streams-impl`

This internal crate provides the `for_streams!` proc macro. Callers should
prefer the `for-streams` crate, which re-exports this macro and provides its
helper functions. For implementation reasons, Rust/Cargo doesn't currently
allow library functions and proc macros to come from a single crate.
