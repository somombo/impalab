---
name: create-component
description: Use this skill to create and configure new Impalab Generator or Executor components. It includes the schema for `impafile.toml` configurations.
---

# Creating Impalab Components

Follow these steps to create new Generator and Executor components for Impalab. This guide covers the structure of `impafile.toml` and the interfaces required for the components.

## The `impafile.toml` Schema

Every component directory must contain an `impafile.toml` file. This file acts as the configuration contract. It registers the component type and defines how Impalab builds and runs the component. You can find the formal JSON Schema in [impafile.schema.json](file:///media/Lean/lean/impalab/.agents/skills/create-component/assets/impafile.schema.json).

The `impafile.toml` schema uses the following fields:

### Fields Table

| Field Name | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `components` | Array of Tables | Yes | A list of components defined within this directory. |
| `components.name` | String | Yes | Unique identifier for the component. |
| `components.type` | String | Yes | The component role. Must be `"generator"` or `"executor"`. |
| `components.build` | Table | No | Instructions on how to compile or build the component. |
| `components.build.command` | String | Yes (if `build` present) | The build tool or executable to run (e.g., `"cargo"`, `"zig"`). |
| `components.build.args` | Array of Strings | No | Arguments passed to the build command (e.g., `["build", "--release"]`). |
| `components.build.working_dir` | String | No | Working directory for the build command, relative to `impafile.toml`. |
| `components.run` | Table | Yes | Instructions on how to run the component executable. |
| `components.run.command` | String | Yes | The executable or runner to run (e.g., `"python3"`, `"./run_binary"`). |
| `components.run.args` | Array of Strings | No | Base arguments passed to the run command (e.g., `["main.py"]`). |
| `components.run.working_dir` | String | No | Working directory for the run command, relative to `impafile.toml`. |

### Example Configurations

#### 1. Compiled Component (e.g., Zig Executor)
```toml
[[components]]
name = "zig-linear-search"
type = "executor"

[components.build]
command = "zig"
args = ["build-exe", "main.zig", "--name", "search_bin", "-O", "ReleaseSafe"]

[components.run]
command = "./search_bin"
```

#### 2. Interpreted Component (e.g., Python Generator)
```toml
[[components]]
name = "python-data-generator"
type = "generator"

[components.run]
command = "python3"
args = ["generate.py"]
```

---

## Component Interfaces

To work with the Impalab orchestrator, your component executables must follow the communication and execution contracts.

### 1. Generator Components

Generators decouple test data generation from task execution.

* **Command Line Arguments**:
  * Must accept a `--seed=<u64>` argument. Impalab passes this argument to guarantee reproducibility.
  * May accept custom arguments to control characteristics of the test data (e.g., `--size=10000`).
* **Standard Output (stdout)**:
  * Must print generated test cases line-by-line to `stdout`.
  * Each line must represent a single test case, beginning with a unique `data_token`, followed by the input data.
  * The rest of the line format is up to you, but the companion executor must know how to parse it.
* **Structured Metadata**:
  * If a `data_token` starts with the `meta:` prefix, the remaining part must be a Base64-encoded, minified JSON string.
  * Impalab decodes this payload and populates the `gen_meta` field in the final output.
  * Keep this metadata payload small (under 1KB) to avoid I/O overhead.
  * Ensure the JSON payload is minified on a single line. Newline characters (`\n`) will break the IPC parser.
* **Standard Error (stderr)**:
  * Use `stderr` for logs and errors. Impalab captures and forwards this stream.

### 2. Executor Components

Executors execute the benchmark work and measure performance.

* **Command Line Arguments**:
  * Must accept any task-specific arguments defined in the json plan configuration.
* **Standard Input (stdin)**:
  * Must read test cases line-by-line from `stdin`, parsing the `data_token` and input data.
* **Standard Output (stdout)**:
  * Must print results to `stdout` in a pipe-delimited format: `metric|data_token[|exec_meta]`
    * **metric**: A numeric outcome (integer or float), such as execution duration (nanoseconds) or memory usage.
    * **data_token**: The unique identifier corresponding to the test case from the generator.
    * **exec_meta** (Optional): A minified, single-line JSON string containing dynamic execution metadata.
* **Metadata Constraints**:
  * Ensure the `exec_meta` JSON payload is minified on a single line. Newline characters (`\n`) will break the IPC parser.
* **Standard Error (stderr)**:
  * Use `stderr` for logs and errors. Impalab captures and forwards this stream.
