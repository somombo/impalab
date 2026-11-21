# impalab-py

Python SDK for [Impalab](../README.md), a language-agnostic framework for orchestrating micro-benchmarks.

## Installation

You can install the SDK using `pip` or `uv`:

```bash
pip install impalab-py
```

To enable pandas and plotting capabilities, install with the `data` extra:

```bash
pip install "impalab-py[data]"
```

To show progress bars in Jupyter notebooks, install with the `notebook` extra:

```bash
pip install "impalab-py[notebook]"
```

## Quick Start

```python
from impalab_py import Impa, Lab

# Initialize the orchestrator
# By default, Impa resolves the executable by checking (in order):
# 1. The `bin_dir` parameter
# 2. The local `.bin` directory
# 3. System `PATH`
# 4. The `IMPALAB_BIN_DIR` environment variable
impa = Impa(
    root_dir="..",
    manifest_filename="impa_manifest.json",
    bin_dir="../.bin"
)

# Build components
impa.build()

# Run a benchmark configuration plan
results = impa.run(
    generator={
        "name": "search-ints-deno",
        "seed": 42,
        "args": ["--size", "10000"]
    },
    reps=5,
    tasks=[
        {"executor": "zig-executors", "args": ["linear_search"]},
        {"executor": "python-executors", "args": ["linear_search_py"]}
    ]
)

# Analyze results with pandas
lab = Lab.from_jsonl(results, metric_label="Duration (ns)")
print(lab.best_of_reps().summary())
```

## Analyzing Results

`Lab` holds results as a pandas dataframe, `lab.df`, with one row per result that `impa` printed. Attributes, generator metadata and executor metadata are flattened into `attr.*`, `gen.*` and `exec.*` columns. A Lab also knows what its metric means: `metric_label` names it on plot axes, and `lower_is_better` says which end of it is best.

A Lab never changes in place. Each step below returns a new Lab with the same metric settings. Every Lab also keeps `lab.raw`, the Lab as it was first built, so you can always get back to every row that was collected.

```python
from impalab_py import Lab

def prepare(df):
    df["gen.size"] = df["gen.size"].astype("int64")
    return df

raw = Lab.from_jsonl(results, metric_label="Duration (ns)")
lab = raw.pipe(prepare).best_of_reps()

print(lab.summary(by="gen.size"))
ax = lab.plot_distributions("gen.size", ylog=True)
fig = lab.plot_trends("gen.size", normalized=True, xlog=True)
fig.show()
```

- `pipe` calls your function on a copy of the dataframe, down to any lists and dicts in its cells, and wraps the result in a new Lab. Use it to cast, derive or select `gen.*` columns.
- `best_of_reps` keeps the best rep of each task on each generated input. Within one run, impalab replays the generator with the same seed on every rep, so `data_token` names the same input in every rep, and keeping the best rep removes system jitter. The best rep has the lowest metric, or the highest when you pass `lower_is_better=False`, for example for accuracy.
- `summary` returns the count, mean, median, min, max and std of each task's metric, grouped by the columns you pass as `by`, with the best median first.
- `plot_distributions` draws a box and strip plot of each task and returns the matplotlib axes. `plot_trends` draws each task's median from `summary` against a column and returns the plotly figure. Neither one shows the plot, so you can adjust it first. Pass `normalized=True` to divide the metric by the x column, for example to get a cost per element.

To analyze a dataframe that is already in memory, wrap it with `Lab(df, metric_label=..., lower_is_better=...)`. It needs at least the `task_label` and `metric` columns, and `best_of_reps` also needs `run_id`, `task_index` and `data_token`.

### Tasks and Runs

`summary` and the plots treat each task configuration, meaning its executor, args and attributes, as a task of its own. Two configurations are never pooled, even when they share an executor and args, and repeated runs of one configuration pool together whatever order their tasks ran in. Attributes set for a whole run count too, so runs with different `experiment_name` attributes, for example, stay apart. When several configurations share an executor and args, their names add the args or attributes they differ in, such as `zig sort [opt=O3]`, and names that would still clash get a number, such as `#2`. Columns you group by don't count towards a task's configuration or its name. The `task_label` column in `lab.df` keeps the plain executor and args.

`best_of_reps` matches reps on `run_id`, `task_index` and `data_token`. From version 0.6.0, `impa` writes the `run_id` of each `impa run` on every row, so results from different runs are never compared, even when their data tokens match. Results from older versions have no `run_id`, and `best_of_reps` refuses them. Add one with `pipe`, using one value per `impa run`. For example, if each run had its own `experiment_name` attribute:

```python
raw = raw.pipe(lambda df: df.assign(run_id=df["attr.experiment_name"]))
```

### Combining Labs

`Lab.concat` stacks Labs, such as the results of several runs:

```python
both = Lab.concat([random_lab, sorted_lab])
print(both.summary(by="attr.experiment_name"))
```

All the Labs must share the same `metric_label` and `lower_is_better`.

## Saving and Loading Results

`Impa.run` keeps results in memory and never writes to disk while benchmarks run. Once a run is over, save the string it returned with `jsonl.write`. Any later notebook can then load it without running `impa` again:

```python
from impalab_py import Lab, jsonl

results = impa.run(...)
jsonl.write("../data/study.jsonl", results)

# Later, possibly in another notebook
raw = Lab.read_jsonl("../data/study.jsonl", metric_label="Duration (ns)")
```

`jsonl.write` saves the text exactly as `impa` printed it. It creates missing folders, replaces an existing file, and compresses the file with gzip when the path ends in `.gz`. It also waits until the data is on disk before it returns. Otherwise the operating system could still be writing the file in the background while your next run takes its measurements. If saving fails, the previous file stays as it was. If only the final sync of the folder fails, it warns instead of raising, because the new file is already saved.

Every row carries its `run_id`, so one file can hold any number of runs. Save each run right after it returns, or all of them together at the end of the notebook:

```python
jsonl.write("../data/study.jsonl", jsonl.join([results1, results2]))
```

To add a run to a file you saved before, join it with what the file already holds: `jsonl.write(path, jsonl.join([jsonl.read(path), results]))`.

`Lab.read_jsonl` takes a file path, a URL, a glob pattern such as `"../data/study-*.jsonl"`, or a list of them, and reads gzip-compressed files too. A path that exists is read as is, even if its name contains glob characters such as `[`.

## Development

Run the full test suite from this directory with the `dev` and `data` extras:

```bash
uv run --isolated --extra dev --extra data pytest
```

`--isolated` builds a temporary environment from uv's cache, so your project `.venv` stays as it is. Tests that need pandas or the plotting libraries skip themselves without the `data` extra, so leaving it out gives a partial run.

To match CI, also pin the Python version and require an up-to-date lockfile:

```bash
uv run --isolated --locked --python 3.13 --extra dev --extra data pytest
```

To check that the SDK still works without the `data` extra, leave it out and list the skipped tests:

```bash
uv run --isolated --extra dev pytest -rs
```

