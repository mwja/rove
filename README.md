# Rove

A less ambitious project for a compiled language.

**Why 'Rove'?**

> _rove_
> to move or travel around an area (without having a particular place you intend to go to)

This language does not intend to solve any goal and is merely a personal
project - hence the 'aimless' aspect of it.

Also because it makes me think of romp and raft (both words for groups of otters
and my first failed attempt at a language was called Otter).

## Steps towards full language-hood

- [x] Type checker
- [x] Functions
- [x] If/else
- [x] Loops
- [x] Constraints (guard, require, ensure)
- [x] Enums
- [x] Switch
- [x] Errors
- [x] enum errors and removing the error union types (`error!T`), `throws X`
- [x] `try! x()`
- [x] `throw`
- [x] `try x() else y` and `try x() else e { ... /* must exit function here */ }`
- [ ] `guard` inside the function body
  - [ ] e.g. `guard x > 0 else { return 0 }`, or `guard ::BiggerThanZero if x > 0 `
- [ ] Fix float division (int/int should produce an int, truncated. not a float)
- [ ] `use X` namespace aliasing.
- [ ] Real booleans
- [ ] Unary operators
- [ ] Make all statements expressions (e.g. `if` and `switch` should return a value)
- [ ] `&&` and `||` with shortcircuiting.
- [ ] Heap allocation
- [ ] Structs
    - [ ] Field access
- [ ] ARC memory management
- [ ] Defer/errdefer
- [ ] Managed/unmanaged strings
- [ ] Arrays
- [ ] Generics
- [ ] Custom linking with C libs

## Use

Rove is a very young language. For now it still depends on a Rust-based set of
runtime helpers. The goal is to eventually weed of this, and even re-write this
compiler in this language.

It is not meant to be used yet, or maybe ever in the future. This is a slower,
more deliberate attempt at developing a compiled language; I have spent days
to weeks trying to build an entire language and ended up with nothing to show
for it. There are no tutorials or guides yet (waiting for heap allocation and strings first)

## Debug artifacts

Pass `--emit` with a comma-separated list to write debug artifacts next to the
input file:

```sh
cargo run -- main.rv --emit ast,typed,clif,opt-clif,cfg,obj
```

- `ast` - `__file.rv.ast`, the parsed AST
- `typed` - `__file.rv.typed`, the AST with type annotations
- `clif` - `__file.rv.clif`, the unoptimized Cranelift IR
- `opt-clif` - `__file.rv.opt.clif`, the optimized Cranelift IR
- `cfg` - `__file.rv.cfg`, a DOT graph of the Cranelift control flow
- `obj` - `__file.rv.o`, the object file (otherwise deleted after linking)

## Testing

Every `.rv` file in `tests/cases/` is compiled and run as a snapshot test with
[insta](https://insta.rs). A snapshot records either the program's exit code,
stdout and stderr, or the compiler diagnostics if it fails to compile.

```sh
cargo test --test snapshots
```

When output changes, the test fails and shows a diff. To review and accept or
reject the changes, install the CLI once and run review:

```sh
cargo install cargo-insta
cargo insta review
```

To add a test, drop a new `.rv` file into `tests/cases/` (prefix it with `err_`
if it is meant to fail), run `cargo insta test --review`, and accept the new
snapshot. Snapshots live in `tests/snapshots/` and should be committed.

## Design principles

The language is evolving and being made, but I will list 'decisions' as I reach
the point I need to make one.

### Danger is highlighted with `!`

Anything that can cause a program to crash must be marked with `!`, such as 
`require!`, `ensure!` or otherwise. It's not yet decided if _callees_ must also
obey this rule (so any function with `require!`/`ensure!` must be called with `!`
as well). A hypothetical example:

```swift
// NOT RUNNABLE CODE - EXAMPLE
func divide(a: int, b: int) -> int
  require! non_zero_divisor: b != 0
{
  return a / b
}

func main() -> int
{
  // without the transitive `!` rule
  let x = divide(1, 0);
  // with it
  let x = divide!(1, 0)
}
```

If it was transitive, functions could also alternatively prove their own safety
with a `safe` keyword, which would allow them to be called without `!`:

```swift
// NOT RUNNABLE CODE - EXAMPLE
func divide(a: int, b: int) -> int
  require! non_zero_divisor: b != 0
{
  return a / b
}

func half(a: int) -> int
  safe
{
  // SAFETY: divide fails if b is 0, but we know b is 2.
  return divide!(a, 2)
}

func main() -> int
{
  let x = divide!(2, 2); // must use `!` because divide is unsafe
  let x = half(2); // no `!` needed, because half is safe
}
```

### Enums may be capitalized or non-capitalized, depending on their function

If an enums purpose is a flag, you may use a lowercase name. If it represents
something else, it should be capitalized.

```swift
enum Color {
  Red,
  Green,
  Blue
}

enum Alignment {
  left,
  center,
  right
}
```

## Windows

Requires the `x86_64-pc-windows-gnu` toolchain to compile the minimally required
runtime at the minute ([runtime](./runtime)). In the future it will not depend
on a precompiled runtime binary.
