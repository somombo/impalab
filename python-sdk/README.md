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
from impalab_py import Impa, LabFromResults

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

# Analyze results with Pandas
lab = LabFromResults(results)
df = lab.to_dataframe()
print(lab.summary())
```

## Exploring Results

`Explorer` compares tasks across a generator parameter, such as the input size. It keeps two dataframes:

- `raw_df` is `Lab.to_dataframe()` unchanged, with one row per rep.
- `clean_df` keeps the best rep of each task on each generated input, then applies your optional `prepare` function.

Use `prepare` to cast, derive or select `gen.*` columns. It only changes `clean_df`, so `raw_df` always matches what `impa` collected.

```python
from impalab_py import Explorer

def prepare(df):
    df["gen.size"] = df["gen.size"].astype("int64")
    return df

explorer = Explorer.from_results(results, prepare=prepare, metric_label="Duration (ns)")
print(explorer.get_clean_data().head())

ax = explorer.plot_distributions("gen.size", ylog=True)
fig = explorer.plot_trends_interactive("gen.size", normalized=True, xlog=True)
fig.show()
```

The best rep is the one with the lowest metric. Pass `lower_is_better=False` to `from_results` when a higher metric is better, such as accuracy. The plots return the matplotlib axes or the plotly figure without showing them, so you can adjust them first.

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

