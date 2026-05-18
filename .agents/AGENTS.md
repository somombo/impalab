Double check the project is in a valid state with:

```
cargo check --all-targets && cargo test && cargo clippy --all-targets -- -D warnings && cargo +nightly fmt --all --check
```