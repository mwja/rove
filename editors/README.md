# Editor support

- `tree-sitter-rove/`: a tree-sitter grammar generated from the rust-sitter
  definitions in `src/syntax.rs`, so editors parse exactly what the compiler
  does. Don't edit `src/` by hand; regenerate it instead.
- `zed/`: a Zed extension with highlighting, outline, brackets, indents, text
  objects and a run button on `func main`.

## Regenerating the grammar

After changing `src/syntax.rs`:

```sh
# regenerate editors/tree-sitter-rove
editors/zed/sync-grammar.sh
# Zed checks the grammar out from git
git commit ...
# point extension.toml at that commit
editors/zed/sync-grammar.sh --rev
```

Then run `zed: rebuild dev extension` (or reinstall it) in Zed.

## Installing in Zed

1. Make sure `rev` in `zed/extension.toml` is a commit that contains
   `editors/tree-sitter-rove` (see above).
2. In Zed, run `zed: install dev extension` and choose `editors/zed`.
3. Open any `.rv` file.

Currently it points to my local file but it will eventually point to 
`https://github.com/mwja/rove`.

## Tasks

`.zed/tasks.json` adds tasks to compile and run the current file, emit debug
artifacts, run the snapshot tests and regenerate the grammar. The run tasks
also show up on the run button next to `func main`.
