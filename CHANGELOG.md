## [0.0.5] - 2026-10-23

### 🚀 Features

- [**breaking**] Add variable mutability (var as opposed to let)
- Add node id resolution inside the resolver (so it shows spans)
- Uses places in prep for structs
- Add partial editor support for zed
- Add basic type resolution and lowering of structs
- Add full struct lowering with more defined mutability rules
- Remove explicit size in rt_alloc and pass tinfo
- *(codegen,ty,runtime)* Add ARC rt_retain and rt_release along with proper counters of ownership

### 🐛 Bug Fixes

- Report correct line numbers in constraint aborts

### ⚙️ Miscellaneous Tasks

- Add wasm to gitignore
- Accept struct leak snapshot as accepted behaviour
- Justfile + changelog generation
- Fix justfile on first run of changelog
- Add skip on release messages and wrap lines back onto one (zed editor seems to insert newlines)
## [0.0.4] - 2026-10-03

### 🚀 Features

- Logical operator short-circuiting
- Use X, use X::Y, use super::X, use x::super
- Turn most statements into expressions and cleanup switch cases (some invalid fallthroughts)

### ⚙️ Miscellaneous Tasks

- Bump version to 0.0.4
## [0.0.3] - 2026-09-30

### 🚀 Features

- Add modules and module loading
- Real boolean types and division now produces the same type of its operands (int/int->int)
- Add ! and - uanary operators

### ⚙️ Miscellaneous Tasks

- Bump version to 0.0.3
## [0.0.2] - 2026-09-27

### 🚀 Features

- Add basic comparitive operators for integers
- Add wrapped expressions
- Add floats and a basic type checker
- Add proper local variable resolution, instead of relying on names
- Add a resolver step for functions and require functions.
- Allowed function returning and added landing pad (not CPU exception)
- Optimise direct function calls over indirect calls
- Added significantly better type checking errors and parsing errors
- Check all paths return and dead code analysis
- Better report double decl failures
- Add loops with empty cleanup blocks for now
- Require and ensure constraints with aborts
- Implicit returns (only on function bodies for now) and error definitions (no using yet)
- Errors exist and are (bugged but) lowered. no check lowering yet
- Handle fallible types correctly in typeck and create error landing pad
- Add simple enums with inferred enum variants
- Type inferrence on returns and readme notes
- Switch cases now exist, with fallthrough mechanics too
- Snapshot tests with cargo-insta, currently broken `throws` syntax
- Replace old error sets with enum-based errors (1-based for now)
- `throw X`
- Add `try x() catch |err| {}` and `try x() catch {}`, both with implicit returns
- Guard and require inside function bodies, and rework guard syntax to keep preconditions consistent
- Add some documentation, guides and plans

### 🐛 Bug Fixes

- Resolved problems where functions couldn't actually use their arguments
- Correct function call precendence

### ⚙️ Miscellaneous Tasks

- Bump version to 0.0.2
## [0.0.1] - 2026-09-08

### ⚙️ Miscellaneous Tasks

- Bump version to 0.0.1
