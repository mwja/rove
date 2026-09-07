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

Rove is a very young language. It currently lacks everything but basic 64 bit 
integer arithmetic (not even division), print statements (with a rust generated
runtime), and declaration/assignment.

It is not meant to be used yet, or maybe ever in the future. This is a slower,
more deliberate attempt at developing a compiled language; I have spent days
to weeks trying to build an entire language and ended up with nothing to show
for it.

## Examples
To run any of the examples, you can do
```sh
# Uses the .rs wrapper to compile and run the example
cargo run --example 2_set_values_1_2 && ./examples/2_set_values_1_2
```

You can also run any test directly on the Rove file
```sh
cargo run -- ./examples/2_set_values_1_2.rv && ./examples/2_set_values_1_2 
```

## Windows

Requires the `x86_64-pc-windows-gnu` toolchain to compile the minimally required
runtime at the minute ([runtime](./runtime)). In the future it will not depend
on a precompiled runtime binary.
