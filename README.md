<p align="center">
  <img src="assets/logo.png" alt="Impalab Logo" width="200"/>
</p>

[![License](https://img.shields.io/badge/license-Apache_2.0-blue.svg)](LICENSE)

# Impalab

Impalab is a language-agnostic framework for orchestrating micro-benchmarks. It allows you to define, build, and run benchmark components written in any language, piping data from a generator to one or more algorithm implementations.

This design makes it simple to perform both:

- **Inter-language benchmarking**: Compare the performance of the same algorithm (e.g., `linear_search`) in Zig vs. Python.
- **Intra-language benchmarking**: Compare the performance of different algorithms (e.g., `linear_search` vs. `binary_search`) within the same language.

The core of Impalab is the `impa` CLI, a Rust-based orchestrator that manages two types of components:

- **Generators**: Programs that generate test data (e.g., random numbers, strings) and print it to `stdout`.
- **Executors**: Programs that read data from `stdin`, run a target function against it, and print performance results to `stdout`.

## Core Concept

Impalab works by decoupling data generation from task execution. You define each component in its own directory with a simple `impafile.toml` that tells Impalab how to build and run it.

1.  **Define**: You create an `impafile.toml` for each `generator` or `executor` component.
2.  **Build**: You run `impa build`, which finds all `impafile.toml` files, executes their optional `[build]` steps, and registers the component's `[run]` command in an `impa_manifest.json`.
3.  **Run**: You run `impa run`, providing a configuration plan specifying which generator and tasks to run. `impa` handles spawning processes, piping `stdout` from the generator to the `stdin` of each executor, and collecting the results.

## The `impafile.toml`

The `impafile.toml` defines the component's name, type, and (most importantly) how to build and run it. The `[run]` block is the key, as `impa` will execute the `command` (assuming it's in the `PATH` or a relative path) and pass the `args`.

**Example 1: Executor (Compiled - Zig)**

This component has a `[components.build]` step to create a binary, and the `[components.run]` command points to the resulting executable.

`my_zig_executors/impafile.toml`:

```toml
[[components]]
name = "zig-executors"
type = "executor"

[components.build]
command = "zig"
args = ["build-exe", "main.zig", "--name", "run_zig", "-O", "ReleaseSmall"]

# 'impa' will execute './run_zig' from this directory
[components.run]
command = "./run_zig"
```

**Example 2: Executor (Interpreted - Python)**

This component has no `[components.build]` step. The `[components.run]` command calls the `python3` interpreter (which `impa` assumes is in the `PATH`) and passes the script name as an argument.

`my_python_executors/impafile.toml`:

```toml
[[components]]
name = "python-executors"
type = "executor"

# No [components.build] step needed!

[components.run]
command = "python3"
args = ["main.py"]
```

**Example 3: Generator (TypeScript - Deno)**

This component uses `deno run` (assuming it's in the `PATH`) to execute the generator script.

`search_gen_deno/impafile.toml`:

```toml
[[components]]
name = "search-ints-deno"
type = "generator"

[components.run]
command = "deno"
args = ["run", "--allow-read", "main.ts"]
```

## Component Interface

To work with Impalab, your component executables must follow a simple interface.

### Generator Executable

- **Must** accept a `--seed=<u64>` argument, which will be provided by `impa`. This ensures that the exact same data is generated for each run, allowing for fair comparisons when testing across different languages.
- **May** accept any number of custom arguments, which are defined in the benchmark configuration run plan. These are used to control the _characteristics_ of the test data (e.g., `--size=10000`).
- **Must** print its generated data to `stdout`. Each line represents a single test case, starting with a unique `data_id` and followed by the input data. It could be JSONL, binary, space delimited, or CSV. The only contract requirement is that the generator encodes a `data_id` that is unique for each line and it encodes the data itself, and that the executor understands how to fully decode and parse that to get back the ID and the data.
- `stderr` will be captured and forwarded by `impa` for logging.

**Example Output (from the TypeScript generator):**

```text
run_1 8 10 5 3 8 1
run_2 4 9 2 7 4 6
```

In this convention, each line is a test case.

- `run_1` is the unique **data_id**.
- `8` is the "needle" to search for.
- `10`, `5`, `3`, `8`, `1` is the "haystack" to search in.

### Executor Executable

- **Must** accept any task-specific arguments passed via the `args` array in the JSON configuration.
- **Must** read test cases line-by-line from `stdin`.
- **Must** understand the data format from the generator (e.g., parse the **data_id**, "needle", and "haystack" from each line).
- **Must** print results to `stdout` in a simple CSV format: `data_id,duration_nanos`. The `data_id` _must_ match the one received from the generator.
- `stderr` will be captured and forwarded by `impa` for logging.

**Example Output (from the Zig executor):**
(This output corresponds to the generator input above for the task with `"args": ["linear_search"]`)

```csv
run_1,450
run_2,455
```

## Workflow Example

### 1\. Project Structure

A typical benchmark project might look like this, with component directories placed in the project root:

```text
my_benchmarks/
├── zig_executors/
│   ├── impafile.toml
│   └── main.zig          # Implements linear_search, binary_search
├── python_executors/
│   ├── impafile.toml
│   └── main.py           # Implements linear_search_py
├── search_ints_gen_deno/
│   ├── impafile.toml
│   ├── main.ts           # Generates lines of "data_id,needle,haystack..."
│   └── deno.json
│
└── impa_manifest.json    # (This file will be generated by impa build)
```

### 2\. Build Components

First, run the `build` command from the project's root directory.

```sh
impa build
```

This command will find all `impafile.toml` files, execute their `[build]` steps, and create `impa_manifest.json` mapping component names and languages to their `[run]` commands. **Note:** This manifest is a build artifact and should typically be added to your `.gitignore` file.

### 3\. Run Benchmarks

Now, run the benchmarks. You will create a JSON configuration plan that instructs the orchestrator on exactly what to execute.

Create a `plan.json` file specifying the generator and the tasks:

`plan.json`:

```json
{
  "generator": {
    "name": "search-ints-deno",
    "args": ["--size", "10000"]
  },
  "labels": {
    "environment": "production",
    "cpu": "x86_64"
  },
  "tasks": [
    {"executor": "zig-executors", "args": ["linear_search"], "labels": {"tier": "high"}},
    {"executor": "zig-executors", "args": ["binary_search"]},
    {"executor": "python-executors", "args": ["linear_search_py"], "labels": {"cpu": "arm64"}}
  ]
}
```

Then provide it to the orchestrator:

```sh
impa run --config plan.json
```

Notice how this single configuration performs both **intra-executor** benchmarking (comparing `linear_search` vs. `binary_search` for the `"zig-executors"` component) and **inter-executor** benchmarking (comparing the Zig `linear_search` against Python's `linear_search_py`).

#### Configuration Labels

You can attach arbitrary metadata to your benchmark results using `labels`. Labels can be defined at the global level (applying to all tasks) or within individual tasks. Task-level labels will be merged with global labels, and can overwrite them if the keys match. In the example above, the `python-executors` task overrides the global `"cpu"` label with `"arm64"`.

### Running "Self-Contained" Executors

If an executor doesn't require generated data (e.g., calculating Fibonacci), you can simply omit the `generator` object from your configuration.

```sh
echo '{"tasks": [{"executor": "zig-executors", "args": ["fib_recursive"]}, {"executor": "zig-executors", "args": ["fib_iterative"]}]}' | \
impa run --config -
```

By omitting the generator, the executor's `stdin` is automatically connected to `/dev/null`. Note the use of `--config -` to pipe the configuration JSON directly from `stdin` into `impa`.

## Benchmark Output & Analysis

`impa` captures the `data_id,duration` CSV output from all tasks and prints it to its own `stdout` as structured, newline-delimited JSON (JSONL). The output includes the `task_index` and any resolved `labels`, providing full traceability.

```json
{"task_index":0,"executor":"zig-executors","args":["linear_search"],"labels":{"environment":"production","cpu":"x86_64","tier":"high"},"data_id":"run_1","duration":450}
{"task_index":1,"executor":"zig-executors","args":["binary_search"],"labels":{"environment":"production","cpu":"x86_64"},"data_id":"run_1","duration":30}
{"task_index":2,"executor":"python-executors","args":["linear_search_py"],"labels":{"environment":"production","cpu":"arm64"},"data_id":"run_1","duration":52000}
```

This JSONL format is designed for easy consumption. While you can pipe it to tools like `jq` for quick queries, the intended use case is to parse it in a data analysis environment. For example, you can easily load the output into a **Jupyter notebook**, parse each line, and build a `pandas.DataFrame` for sophisticated analysis and visualization.

## Command-Line Reference

### `impa build`

Scans for `impafile.toml` files, runs their build commands, and creates a JSON manifest.

- `--components-dir <PATH>`: The root directory containing component subdirectories. (Default: `.`)
- `--root-dir <PATH>`: The output directory for the build manifest. (Default: `.`)
- `--manifest-filename <PATH>`: The filename for the build manifest.
- `--include <LIST>`: Comma-separated list of components to include in the build.
- `--exclude <LIST>`: Comma-separated list of components to exclude from the build.

### `impa run`

Runs the benchmark using the specified components and manifest.

**Key Arguments:**

- `--config <PATH>`: Path to a JSON configuration file defining the benchmarking parameters (generator and tasks array). Use `-` to read from `stdin`.
- `--root-dir <PATH>`: Output path for the build manifest. Path to the build manifest (generated by the 'build' command) [default: .]
- `--manifest-filename <PATH>`: Path to the build manifest.

**Override Arguments:**
You can modify the configuration hierarchy or component specifications on the fly using `--set`. *Note: Arrays (like the `tasks` list or `args` array) cannot be overridden via `--set`.*

- `--set <KEY=VALUE>`: Overrides configuration variables in the hierarchical config.
  - Example: `--set generator.name=py-gen`
  - Example: `--set generator.seed=42`
  - Example: `--set components.my-python-exec.command=/opt/python3/bin/python`

## Logging

Logging is configured via environment variables:

- `RUST_LOG`: Sets the log level (e.g., `RUST_LOG=info`, `RUST_LOG=debug`). Defaults to `info`.
- `BENCH_LOG_FILE`: If set, logs are written to this file instead of `stderr`.

## License

This project is licensed under the Apache 2.0 License. See the [LICENSE](/LICENSE) file for details.

## Contributing

Contributions are welcome\! Please feel free to open an issue or submit a pull request.
