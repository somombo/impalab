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

## Quick Start

```python
from impalab_py import Impa, LabFromResults

# Initialize the orchestrator
impa = Impa(
    root_dir="..",
    manifest_filename="impa_manifest.json",
    impa_path="../target/debug"
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

## Development

To run the test suite, use `uv` with the `dev` extra:

```bash
uv run --extra dev pytest
```

