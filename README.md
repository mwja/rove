# Rove

A less ambitious project for a compiled language.

**Why 'Rove'?**

> _rove_
> to move or travel around an area (without having a particular place you intend to go to)

This language does not intend to solve any goal and is merely a personal
project - hence the 'aimless' aspect of it.

Also because it makes me think of romp and raft (both words for groups of otters
and my first failed attempt at a language was called Otter).

## Use

Rove is a very young language. For now it still depends on a Rust-based set of 
runtime helpers. The goal is to eventually weed of this, and even re-write this
compiler in this language.

It is not meant to be used yet, or maybe ever in the future. This is a slower,
more deliberate attempt at developing a compiled language; I have spent days
to weeks trying to build an entire language and ended up with nothing to show
for it.

## Examples

To run any of the examples, you can do

```sh
# Uses the .rs wrapper to compile and run the example
cargo run --example 7_basic_func && ./examples/7_basic_func
```

You can also run any test directly on the Rove file

```sh
cargo run -- ./examples/7_basic_func.rv && ./examples/7_basic_func
```

For now, it generates a few artifacts, namely

- `file.rv.o` - The object file
- `file.rv.typed` - The AST typed notation of the code
- `file.rv.clif` - The (unoptimized) Cranelift representation
- `file.rv.opt.clif` - The (optimized) Cranelift representation
- `file.rv.cfg` - The DOT graph of the cranelift IR.

This is for debugging and will likely be toned down as development moves on.

## Windows

Requires the `x86_64-pc-windows-gnu` toolchain to compile the minimally required
runtime at the minute ([runtime](./runtime)). In the future it will not depend
on a precompiled runtime binary.
