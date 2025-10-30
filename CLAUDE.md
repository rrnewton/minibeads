
Let's use debug mode `cargo build` for all testing and iterative development (not `--release`).


Coding conventions
========================================

PREFER STRONG TYPES. Do not use "u32" or "String" where you can have a more specific type or at least a type alias. "String" makes it very unclear which values are legal. We want explicit Enums to lock down the possibilities for our state, and we want separate types for numerical IDs and distinct, non-overlapping uses of basic integers.

Delete trailing spaces. Don't leave empty lines that consist only of whitespace. (Double newline is fine.)

Add README.md files for every major subdirectory/subsystem.  For example `src/core`, `src/game`, etc.

Read the PROJECT_VISION description of coding conventions we should follow for high-performance Rust (unboxing, minimizing allocation, etc). In particular, adhere to the below programming patterns / avoid anti-patterns, which generally fall under the principle of "zero copy":

- Avoid clone: instead take a temporary reference to the object and manage lifetimes appropriately.
- Avoid collect: instead take an iterator with references to the original collection without copying.

Read OPTIMIZATION.md for more details.

SAFETY! This is a safe-rust project. We will not introduce the `unsafe` keyword unless we have a VERY good reason and with significant advanced planning.