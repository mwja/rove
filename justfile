# Tools some recipes need, beyond cargo:
#   cargo-watch    watch
#   cargo-insta    review
#   git-cliff      changelog (and so version)
#   cargo-release  version
#
#   cargo install cargo-watch cargo-insta git-cliff cargo-release

default: test

build: runtime
    cargo build

# the static library compiled programs link against
runtime:
    cargo build --release --manifest-path runtime/Cargo.toml

test: runtime
    cargo test

# rerun the tests on every save
watch:
    cargo watch -w src -w runtime/src -w tests -x test

# accept or reject changed snapshots
review:
    cargo insta review

# compile a .rv file and run it, e.g. `just run tests/cases/point.rv`
run file: build
    ./target/debug/rove {{ file }}
    ./{{ without_extension(file) }}

# same as run, but report live objects at exit
leaks file: build
    ./target/debug/rove {{ file }}
    ROVEFLAGS=debug_leaks ./{{ without_extension(file) }}

# use the repo's git hooks (conventional commit check)
install-hook:
    git config core.hooksPath .githooks

remove-hook:
    git config --unset core.hooksPath

## Release stuff

changelog version:
    git-cliff --output CHANGELOG.md --tag {{ version }}

version level:
    cargo release --sign-tag --no-publish --no-push --execute {{ level }}
