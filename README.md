[![License](https://img.shields.io/badge/license-Apache_2.0-blue.svg)](LICENSE)

# Impalab

Impalab is a language-agnostic framework for orchestrating micro-benchmarks. It allows you to define, build, and run benchmark components written in any language, piping data from a generator to one or more algorithm implementations.

This design makes it simple to perform both:

* **Inter-language benchmarking**: Compare the performance of the same algorithm (e.g., `linear_search`) in Zig vs. Python.
* **Intra-language benchmarking**: Compare the performance of different algorithms (e.g., `linear_search` vs. `binary_search`) within the same language.

The core of Impalab is the `impa` CLI, a Rust-based orchestrator that manages two types of components:

* **Generators**: Programs that generate test data (e.g., random numbers, strings) and print it to `stdout`.
* **Algorithms**: Programs that read data from `stdin`, run one or more named functions against it, and print performance results to `stdout`.

## Core Concept

Impalab works by decoupling data generation from algorithm execution. You define each component in its own directory with a simple `impafile.toml` that tells Impalab how to build and run it.

1.  **Define**: You create an `impafile.toml` for each `generator` or `algorithm` component.
2.  **Build**: You run `impa build`, which finds all `impafile.toml` files, executes their optional `[build]` steps, and registers the component's `[run]` command in a `impa_manifest.json`.
3.  **Run**: You run `impa run`, specifying which generator to use and which algorithms to test. `impa` handles spawning processes, piping `stdout` from the generator to the `stdin` of each algorithm, and collecting the results.

## The `impafile.toml`

The `impafile.toml` defines the component's name, type, and—most importantly—how to build and run it. The `[run]` block is the key, as `impa` will execute the `command` (assuming it's in the `PATH` or a relative path) and pass the `args`.

**Example 1: Algorithm (Compiled - Zig)**

This component has a `[build]` step to create a binary, and the `[run]` command points to the resulting executable.

`my_zig_algos/impafile.toml`:
```toml
name = "zig-algos"
type = "algorithm"
language = "zig"

[build]
command = "zig"
args = ["build-exe", "main.zig", "--name", "run_zig", "-O", "ReleaseSmall"]

# 'impa' will execute './run_zig' from this directory
[run]
command = "./run_zig"
```

**Example 2: Algorithm (Interpreted - Python)**

This component has no `[build]` step. The `[run]` command calls the `python3` interpreter (which `impa` assumes is in the `PATH`) and passes the script name as an argument.

`my_python_algos/impafile.toml`:

```toml
name = "python-algos"
type = "algorithm"
language = "python"

# No [build] step needed!

[run]
command = "python3"
args = ["main.py"]
```

**Example 3: Generator (TypeScript - Deno)**

This component uses `deno run` (assuming it's in the `PATH`) to execute the generator script.

`search_gen_deno/impafile.toml`:

```toml
name = "search-ints-deno"
type = "generator"

[run]
command = "deno"
args = ["run", "--allow-read", "main.ts"]
```

## Component Interface

To work with Impalab, your component executables must follow a simple interface.

### Generator Executable

  * **Must** accept a `--seed=<u64>` argument, which will be provided by `impa`. This ensures that the exact same data is generated for each run, allowing for fair comparisons when testing across different languages.
  * **May** accept any number of custom arguments, which are passed through by the `impa run` command. These are used to control the *characteristics* of the test data (e.g., `--size=10000`).
  * **Must** print its generated data to `stdout`. Each line represents a single test case, starting with a unique `id`.
  * `stderr` will be captured and forwarded by `impa` for logging.

**Example Output (from the TypeScript generator):**

```text
run_1 8 10 5 3 8 1
run_2 4 9 2 7 4 6
```

In this convention, each line is a test case.

  * `run_1` is the unique **ID**.
  * `8` is the "needle" to search for.
  * `10 5 3 8 1` is the "haystack" to search in.

### Algorithm Executable

  * **Must** accept a `--functions=<list>` argument (e.g., `--functions=linear_search,binary_search`).
  * **Must** read test cases line-by-line from `stdin`.
  * **Must** understand the data format from the generator (e.g., parse the **ID**, "needle", and "haystack" from each line).
  * **Must** print results to `stdout` in a simple CSV format: `id,function_name,duration_nanos`. The `id` *must* match the one received from the generator.
  * `stderr` will be captured and forwarded by `impa` for logging.

**Example Output (from the Zig algorithm):**
(This output corresponds to the generator input above)

```csv
run_1,linear_search,450
run_1,binary_search,30
run_2,linear_search,455
run_2,binary_search,31
```

## Workflow Example

### 1\. Project Structure

A typical benchmark project might look like this, with component directories placed in the project root:

```text
my_benchmarks/
├── zig_algos/
│   ├── impafile.toml
│   └── main.zig          # Implements linear_search, binary_search
├── python_algos/
│   ├── impafile.toml
│   └── main.py           # Implements linear_search_py
├── search_ints_gen_deno/
│   ├── impafile.toml
│   ├── main.ts           # Generates lines of "id needle haystack..."
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

Now, run the benchmarks. This command will use the `search-ints-deno` generator, pipe its output to both the `zig` and `python` algorithm executables, and pass the `--size 10000` argument to the generator.

```sh
impa run \
    --generator "search-ints-deno" \
    --algorithms '{"zig": ["linear_search", "binary_search"], "python": ["linear_search_py"]}' \
    -- \
    --size 10000
```

  * `--generator "search-ints-deno"`: Use the generator named in its `impafile.toml`.
  * `--algorithms '...'`: A JSON map of `language` to a list of function names to run.
  * `--`: All arguments after this are passed directly to the generator (`search-ints-deno`).

Notice how this single command performs both **intra-language** benchmarking (comparing `linear_search` vs. `binary_search` for the `"zig"` language) and **inter-language** benchmarking (comparing the Zig `linear_search` against Python's `linear_search_py`).

### Running "Self-Contained" Algorithms

If an algorithm doesn't require generated data (e.g., calculating Fibonacci), you can use `generator = "none"`.

```sh
impa run \
    --generator "none" \
    --algorithms '{"zig": ["fib_recursive", "fib_iterative"]}'
```

In this mode, the algorithm's `stdin` is connected to `/dev/null`.

## Benchmark Output & Analysis

`impa` captures the `id,func,duration` CSV output from all algorithm components and prints it to its own `stdout` as structured, newline-delimited JSON (JSONL).

```json
{"id":"run_1","language":"zig","function_name":"linear_search","duration":450}
{"id":"run_1","language":"zig","function_name":"binary_search","duration":30}
{"id":"run_1","language":"python","function_name":"linear_search_py","duration":52000}
{"id":"run_2","language":"zig","function_name":"linear_search","duration":455}
{"id":"run_2","language":"zig","function_name":"binary_search","duration":31}
{"id":"run_2","language":"python","function_name":"linear_search_py","duration":52150}
```

This JSONL format is designed for easy consumption. While you can pipe it to tools like `jq` for quick queries, the intended use case is to parse it in a data analysis environment. For example, you can easily load the output into a **Jupyter notebook**, parse each line, and build a `pandas.DataFrame` for sophisticated analysis and visualization.

## Command-Line Reference

### `impa build`

Scans for `impafile.toml` files, runs their build commands, and creates a JSON manifest.

  * `--components-dir <PATH>`: The root directory containing component subdirectories. (Default: `.`)
  * `--manifest-path <PATH>`: The output path for the build manifest. (Default: `impa_manifest.json`)

### `impa run`

Runs the benchmark using the specified components and manifest.

**Key Arguments:**

  * `--algorithms <JSON_STRING>`: (Required) A JSON string mapping languages to a list of function names to run.
      * Example: `'{"zig": ["linear_search", "binary_search"], "python": ["linear_search_py"]}'`
  * `--generator <NAME>`: (Required) The name of the generator component to use (must match a name in the manifest), or `none` for self-contained algorithms.
  * `--manifest-path <PATH>`: Path to the build manifest. (Default: `impa_manifest.json`)
  * `--seed <u64>`: (Optional) A specific seed for the random number generator.
  * `[generator_args]...`: Any arguments after `--` are passed directly to the generator executable.

**Override Arguments:**
You can bypass the manifest file by providing direct paths to executables:

  * `--generator-override-path <PATH>`: Use this executable for the generator.
  * `--algorithm-override-paths <JSON_MAP>`: A JSON string mapping a language to a specific executable path.
      * Example: `'{"zig": "./my_zig_exe", "python": "./main.py"}'`

## Logging

Logging is configured via environment variables:

  * `RUST_LOG`: Sets the log level (e.g., `RUST_LOG=info`, `RUST_LOG=debug`). Defaults to `info`.
  * `BENCH_LOG_FILE`: If set, logs are written to this file instead of `stderr`.

## License

This project is licensed under the Apache 2.0 License. See the [LICENSE](/LICENSE) file for details.

## Contributing

Contributions are welcome\! Please feel free to open an issue or submit a pull request.
