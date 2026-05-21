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
- **Must** print its generated data to `stdout`. Each line represents a single test case, starting with a unique `data_token` and followed by the input data. It could be JSONL, binary, space delimited, or CSV. The only contract requirement is that the generator encodes a `data_token` that is unique for each line and it encodes the data itself, and that the executor understands how to fully decode and parse that to get back the token and the data.
- `stderr` will be captured and forwarded by `impa` for logging.

**Example Output (from the TypeScript generator):**

```text
run_1 8 10 5 3 8 1
run_2 4 9 2 7 4 6
```

In this convention, each line is a test case.

- `run_1` is the unique **data_token**.
- `8` is the "needle" to search for.
- `10`, `5`, `3`, `8`, `1` is the "haystack" to search in.

### Advanced Metadata (data_token)

While `data_token` is typically a simple string (e.g., `run_1`), Impalab supports embedding structured metadata directly into the `data_token`. If a `data_token` starts with the `meta:` prefix, the remaining part is expected to be a **Base64-encoded JSON string**.

When `impa` encounters such a `data_token`, it will:
1. Strip the `meta:` prefix.
2. Base64-decode the content.
3. Parse it as JSON.
4. Populate the `gen_meta` field in the final output with the resulting JSON object, while preserving the raw string in `data_token`.

**Example Generator Output:**
```text
meta:eyJzaXplIjogMTAwfSwxLDIsMyw0
```
*(Where `eyJzaXplIjogMTAwfS` is `{"size": 100}` in Base64)*

**Final JSONL Output:**
```
{"task_index":0,...,"data_token":"meta:eyJzaXplIjogMTAwfS","gen_meta":{"size": 100},"metric":42}
```

> [!WARNING]
> **Keep Metadata Small**
> To avoid skewing performance metrics due to excessive I/O overhead between the generator and executor, it is strongly recommended to keep the JSON payload small (e.g., < 1KB).

> [!WARNING]
> **Minified JSON Constraint**
> All JSON output by components (both `gen_meta` and `exec_meta`) MUST be minified onto a single line. Newline characters (`\n`) within the JSON payload will break the orchestrator's IPC stream parser.

### Executor Executable

- **Must** accept any task-specific arguments passed via the `args` array in the JSON configuration.
- **Must** read test cases line-by-line from `stdin`.
- **Must** understand the data format from the generator (e.g., parse the **data_token**, "needle", and "haystack" from each line).
- **Must** print results to `stdout` in a pipe-delimited format: `metric|data_token[|exec_meta]`.
    - **metric**: Any numeric outcome (integer or float).
    - **data_token**: The unique identifier from the generator.
    - **exec_meta** (Optional): Any valid JSON (primitives, arrays, objects) containing dynamic execution metadata.
- `stderr` will be captured and forwarded by `impa` for logging.

> [!NOTE]
> **What is a Metric?**
> A `metric` can be any valid JSON number (integer or float). While frequently used for execution time (nanoseconds), it can also represent memory usage (bytes), accuracy (0.0 - 1.0), cost, or any other numeric outcome of your task.

**Example Output (from the Zig executor):**
(This output corresponds to the generator input above for the task with `"args": ["linear_search"]`)

```text
450|run_1
455|run_2|{"converged":true,"iters":10}
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
│   ├── main.ts           # Generates lines of "data_token,needle,haystack..."
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
  "reps": 5,
  "attributes": {
    "environment": "production",
    "threads": 8,
    "cpu": "x86_64"
  },
  "tasks": [
    {"executor": "zig-executors", "args": ["linear_search"], "attributes": {"tier": "high", "simd": true}},
    {"executor": "zig-executors", "args": ["binary_search"], "reps": 10},
    {"executor": "python-executors", "args": ["linear_search_py"], "attributes": {"cpu": "arm64"}}
  ]
}
```

Then provide it to the orchestrator:

```sh
impa run --config plan.json
```

Notice how this single configuration performs both **intra-executor** benchmarking (comparing `linear_search` vs. `binary_search` for the `"zig-executors"` component) and **inter-executor** benchmarking (comparing the Zig `linear_search` against Python's `linear_search_py`).

#### Multiple Executions (`reps`)

The `reps` field allows you to execute each task multiple times to gather more statistically significant data. You can set a global `reps` value or override it for specific tasks. In the example above, the `binary_search` task will run 10 times, while others will run 5 times (inheriting from the global `reps`).

> [!IMPORTANT]
> **Generator Determinism - The Trust Contract**
> The integrity of the `reps` feature relies entirely on the generator component producing the **exact same data stream** every time it receives the same `--seed`. If a generator ignores the seed and produces random data (e.g., using `random.random()` without seeding), each repetition will benchmark a different dataset, making the results incomparable. Component authors MUST ensure their generators honor the `--seed` contract.

#### Configuration Attributes

You can attach arbitrary metadata to your benchmark results using `attributes`. Attributes can be defined at the global level (applying to all tasks) or within individual tasks. Task-level attributes will be merged with global attributes, and can overwrite them if the keys match. In the example above, the `python-executors` task overrides the global `"cpu"` attribute with `"arm64"`.

Unlike standard attributes which are often restricted to strings, Impalab attributes support any valid JSON (primitives, arrays, objects). This allows you to pass structured configuration directly through to your analysis tools without manual type casting.

> [!IMPORTANT]
> **RFC 7396 Trade-offs**
> Impalab attributes utilize JSON Merge Patch (RFC 7396) semantics for configuration overriding. This means that setting an attribute key to `null` in a task definition acts as a deletion operator, removing that key from the inherited global attributes. Consequently, `null` cannot be passed as a literal value for an attribute.

### Running "Self-Contained" Executors

If an executor doesn't require generated data (e.g., calculating Fibonacci), you can simply omit the `generator` object from your configuration.

```sh
echo '{"tasks": [{"executor": "zig-executors", "args": ["fib_recursive"]}, {"executor": "zig-executors", "args": ["fib_iterative"]}]}' | \
impa run --config -
```

By omitting the generator, the executor's `stdin` is automatically connected to `/dev/null`. Note the use of `--config -` to pipe the configuration JSON directly from `stdin` into `impa`.

## Benchmark Output & Analysis

`impa` captures the pipe-delimited output from all tasks and prints it to its own `stdout` as structured, newline-delimited JSON (JSONL). The output includes the `task_index`, the `rep_index`, any resolved `attributes`, and optional metadata from both the generator and the executor. To keep the output clean, empty fields (such as `args` or `attributes` when they are empty) and missing metadata fields are omitted from the JSON object.

```json
{"task_index":0,"executor":"zig-executors","args":["linear_search"],"rep_index":0,"attributes":{"environment":"production","threads":8,"cpu":"x86_64","tier":"high","simd":true},"data_token":"run_1","metric":450}
{"task_index":1,"executor":"zig-executors","args":["binary_search"],"rep_index":0,"attributes":{"environment":"production","threads":8,"cpu":"x86_64"},"data_token":"run_1","metric":30}
{"task_index":2,"executor":"python-executors","args":["linear_search_py"],"rep_index":0,"attributes":{"environment":"production","threads":8,"cpu":"arm64"},"data_token":"run_1","exec_meta":{"converged":true},"metric":52000}
```

This JSONL format is designed for easy consumption. While you can pipe it to tools like `jq` for quick queries, the intended use case is to parse it in a data analysis environment like a **Jupyter notebook** using Python and Pandas.

#### Data Science Workflow (Pandas)

When dealing with nested JSON arrays and objects in your `attributes`, `gen_meta`, or `exec_meta`, you can use `pandas.json_normalize()` to automatically flatten the nested metadata into a clean DataFrame.

```python
import pandas as pd
import json

with open("results.jsonl") as f:
    data = [json.loads(line) for line in f]

# Flatten nested JSON structures (e.g. attributes.cpu, exec_meta.converged)
df = pd.json_normalize(data)
print(df.head())
```

> [!TIP]
> **Best Practices for Metadata**
> While Impalab supports complex, nested JSON, we recommend keeping metadata as flat and consistent as possible to avoid heterogeneous typing issues downstream when analyzing the data.

## Command-Line Reference

### `impa build`

Scans for `impafile.toml` files, runs their build commands, and creates a JSON manifest.

- `--components-dir <PATH>`: The root directory containing component subdirectories. (Default: `.`)
- `--root-dir <PATH>`: The output directory for the build manifest. (Default: `.`)
- `--manifest-filename <PATH>`: The filename for the build manifest.
- `--include <LIST>`: Comma-separated list of components to execute build steps for. Filtered-out components will still be registered in the manifest, but their build steps will not run.
- `--exclude <LIST>`: Comma-separated list of components to exclude from build step execution. Excluded components will still be registered in the manifest, but their build steps will not run.

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
