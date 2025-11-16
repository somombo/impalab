
# Contributing to Impalab

First off, thank you for considering contributing! We welcome any help, from reporting bugs to submitting new features.

Please take a moment to review this document to make the contribution process easy and effective for everyone involved.

## How to Contribute

There are many ways to contribute:

* **Reporting Bugs:** If you find a bug, please [open an issue](https://github.com/somombo/impalab/issues) and provide as much detail as possible, including your OS, the command you ran, and the full log output.
* **Suggesting Enhancements:** Have an idea? We'd love to hear it. Open an issue to discuss your enhancement.
* **Submitting Pull Requests:** If you want to fix a bug or add a new feature, feel free to open a Pull Request.

## Key Project Concepts

To contribute code, it helps to understand the core architecture.

### The `impa` CLI

The `impa` binary is the main orchestrator, built in Rust. It has two primary commands:

1.  **`impa build`**: This command crawls the component directories, finds all `impafile.toml` files, runs their optional `[build]` step, and saves the `[run]` commands to the `impa_manifest.json`.
2.  **`impa run`**: This command reads the `impa_manifest.json` (or CLI overrides), spawns the chosen `generator` process, and pipes its `stdout` to the `stdin` of one or more `algorithm` processes. It captures the `stdout` from the algorithms and prints it as JSONL.

### The `impafile.toml` Contract

The `impafile.toml` is the "contract" that defines a component.

```toml
# A unique name for this component.
name = "my-python-generator"

# The type: "generator" or "algorithm"
type = "generator"

# (For 'algorithm' type only) The language key.
# This MUST match the key used in the `impa run --algorithms` JSON.
language = "python"

# (Optional) The build step to run with `impa build`.
[build]
command = "python3"
args = ["-m", "pip", "install", "-r", "requirements.txt"]

# (Required) The command to execute for `impa run`.
[run]
command = "python3"
args = ["./gen.py"]
```

### Component Interfaces

For the `impa` orchestrator to work, your component must respect its interface:

  * **Generators** MUST:

      * Accept a `--seed=<u64>` argument.
      * Accept any passthrough arguments (e.g., `--size=1000`).
      * Print test cases to `stdout`, one per line.
      * Each line MUST start with a unique `id` (e.g., `test_1,...`).

  * **Algorithms** MUST:

      * Accept a `--functions=<list>` argument (e.g., `--functions=func1,func2`).
      * Read test cases line-by-line from `stdin`.
      * For each line, parse the `id` from the generator.
      * For each function in `--functions`, run the benchmark and print the result to `stdout`.
      * The output format MUST be `id,function_name,duration_nanos\n`.

## Development Setup

1.  [Fork](https://github.com/somombo/impalab/fork) and clone the repository.
2.  Install the stable Rust toolchain: `rustup install stable`
3.  Install the nightly Rust toolchain (for formatting): `rustup install nightly`
4.  Run the tests to confirm everything is working: `cargo test`

### Code Style

This project uses `rustfmt` with the nightly toolchain. Before committing, please run the formatter:

```sh
cargo +nightly fmt
```

Our CI will check this, so running it locally saves time.

### Running Tests

We use integration tests in the `tests/` directory. All tests can be run with:

```sh
cargo test
```

If you are adding a new feature, please try to add a corresponding test to `tests/cli.rs`.
