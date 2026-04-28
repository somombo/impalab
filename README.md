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
2.  **Build**: You run `impa build`, which finds all `impafile.toml` files, executes their optional `[build]` steps, and registers the component's `[run]` command in a `impa_manifest.json`.
3.  **Run**: You run `impa run`, specifying which generator to use and which tasks to run. `impa` handles spawning processes, piping `stdout` from the generator to the `stdin` of each executor, and collecting the results.

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
- **May** accept any number of custom arguments, which are passed through by the `impa run` command. These are used to control the _characteristics_ of the test data (e.g., `--size=10000`).
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
└── impa_manifest.json        (This file will be generated)
```

### 2\. Build Components

First, run the `build` command from the project's root directory.

```sh
impa build
```

This command will find all `impafile.toml` files, execute their `[build]` steps, and create `impa_manifest.json` mapping component names and languages to their `[run]` commands. **Note:** This manifest is a build artifact and should typically be added to your `.gitignore` file.

### 3\. Run Benchmarks

Now, run the benchmarks. This command will use the `search-ints-deno` generator, pipe its output to the `zig-executors` and `python-executors` executables, and pass the `--size 10000` argument to the generator.

```sh
impa run \
    --generator "search-ints-deno" \
    --tasks '[{"executor": "zig-executors", "args": ["linear_search"]}, {"executor": "zig-executors", "args": ["binary_search"]}, {"executor": "python-executors", "args": ["linear_search_py"]}]' \
    -- \
    --size 10000
```

- `--generator "search-ints-deno"`: Use the generator named in its `impafile.toml`.
- `--tasks '...'`: A JSON array specifying the tasks to run, including the executor name and any required arguments.
- `--`: All arguments after this are passed directly to the generator (`search-ints-deno`).

Notice how this single command performs both **intra-executor** benchmarking (comparing `linear_search` vs. `binary_search` for the `"zig-executors"` component) and **inter-executor** benchmarking (comparing the Zig `linear_search` against Python's `linear_search_py`).

### Running "Self-Contained" Executors

If an executor doesn't require generated data (e.g., calculating Fibonacci), you can use `generator = "none"`.

```sh
impa run \
    --generator "none" \
    --tasks '[{"executor": "zig-executors", "args": ["fib_recursive"]}, {"executor": "zig-executors", "args": ["fib_iterative"]}]'
```

In this mode, the executor's `stdin` is connected to `/dev/null`.

## Benchmark Output & Analysis

`impa` captures the `data_id,duration` CSV output from all tasks and prints it to its own `stdout` as structured, newline-delimited JSON (JSONL). The output includes the `task_index`, providing full traceability.

```json
{"task_index":0,"executor":"zig-executors","args":["linear_search"],"data_id":"run_1","duration":450}
{"task_index":1,"executor":"zig-executors","args":["binary_search"],"data_id":"run_1","duration":30}
{"task_index":2,"executor":"python-executors","args":["linear_search_py"],"data_id":"run_1","duration":52000}
{"task_index":0,"executor":"zig-executors","args":["linear_search"],"data_id":"run_2","duration":455}
{"task_index":1,"executor":"zig-executors","args":["binary_search"],"data_id":"run_2","duration":31}
{"task_index":2,"executor":"python-executors","args":["linear_search_py"],"data_id":"run_2","duration":52150}
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

- `--tasks <JSON_STRING>`: (Required) A JSON array specifying the tasks to run, including the executor and any required arguments.
  - Example: `'[{"executor": "zig-executors", "args": ["linear_search"]}]'`
- `--generator <NAME>`: (Required) The name of the generator component to use (must match a name in the manifest), or `none` for self-contained executors.
- `--root-dir <PATH>`: Output path for the build manifest. Path to the build manifest (generated by the 'build' command) [default: .]
- `--manifest-filename <PATH>`: Path to the build manifest.
- `--seed <u64>`: (Optional) A specific seed for the random number generator.
- `[generator_args]...`: Any arguments after `--` are passed directly to the generator executable.

**Override Arguments:**
You can modify the components from the manifest file by providing overrides:

- `--component-overrides <JSON_MAP>`: A JSON string mapping a component name to an override structure.
  - Example: `'{"my-python-exec": {"type": "executor", "command": "/opt/python3/bin/python", "args": ["exec.py"]}}'`

## Logging

Logging is configured via environment variables:

- `RUST_LOG`: Sets the log level (e.g., `RUST_LOG=info`, `RUST_LOG=debug`). Defaults to `info`.
- `BENCH_LOG_FILE`: If set, logs are written to this file instead of `stderr`.

## License

This project is licensed under the Apache 2.0 License. See the [LICENSE](/LICENSE) file for details.

## Contributing

Contributions are welcome\! Please feel free to open an issue or submit a pull request.
