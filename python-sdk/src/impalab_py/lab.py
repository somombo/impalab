"""Analyze impalab results as pandas dataframes.

A :class:`Lab` holds one row per result that ``impa`` printed, flattened into
columns, together with what the metric means. Labs are values: no method
changes the Lab it is called on, and every transform returns a new Lab.
"""

from __future__ import annotations

import json
from collections import Counter
from collections.abc import Callable, Iterable, Sequence
from typing import TYPE_CHECKING, Any, Self

from . import jsonl

if TYPE_CHECKING:
    import pandas as pd
    from matplotlib.axes import Axes
    from plotly.graph_objects import Figure

REQUIRED_COLUMNS = ("task_label", "metric")
REP_KEYS = ("run_id", "task_index", "data_token")


def _import_pandas():
    try:
        import pandas as pd
    except ImportError as e:
        raise ImportError(
            "pandas is required to use Lab. "
            "Install it using: pip install pandas"
        ) from e
    return pd


def _to_category(series: pd.Series) -> pd.Series:
    """Return ``series`` as a category, unless it holds lists or dicts, which can't be categories."""
    pd = _import_pandas()
    if isinstance(series.dtype, pd.CategoricalDtype) or series.map(lambda v: isinstance(v, (list, dict))).any():
        return series
    return series.astype("category")


def _copy_containers(value: Any) -> Any:
    if isinstance(value, list):
        return [_copy_containers(v) for v in value]
    if isinstance(value, dict):
        return {k: _copy_containers(v) for k, v in value.items()}
    return value


def _is_missing(value: Any) -> bool:
    pd = _import_pandas()
    return pd.api.types.is_scalar(value) and bool(pd.isna(value))


def _json_key(value: Any) -> str | None:
    """Encode a cell as JSON, so a list such as ``[2, 3]`` and the string ``"[2, 3]"`` get different keys."""
    return None if _is_missing(value) else json.dumps(value, sort_keys=True, default=str)


def _distinct(names: dict[Any, str]) -> dict[Any, str]:
    """Number the names that several configurations still share, such as an attribute of 3 and one of "3"."""
    counts = Counter(names.values())
    taken = set()
    distinct = {}
    for config, name in names.items():
        if counts[name] > 1:
            n = 1
            while f"{name} #{n}" in counts or f"{name} #{n}" in taken:
                n += 1
            name = f"{name} #{n}"
        taken.add(name)
        distinct[config] = name
    return distinct


def _records_to_frame(records: list[dict[str, Any]]) -> pd.DataFrame:
    """Flatten impa result records into one row each.

    ``attributes`` become ``attr.*`` columns, with string ones stored as
    categories, ``gen_meta`` becomes ``gen.*`` and ``exec_meta`` becomes
    ``exec.*``. ``task_label`` names each task by its executor and args.
    """
    pd = _import_pandas()

    rows = []
    string_attr_cols = set()
    for r in records:
        row = r.copy()

        executor = row.get("executor", "")
        args = row.get("args", [])
        if isinstance(args, list):
            args_str = " ".join(str(arg) for arg in args)
        else:
            args_str = str(args)
        row["task_label"] = f"{executor} {args_str}" if args_str else executor

        attrs = row.get("attributes")
        if isinstance(attrs, dict):
            del row["attributes"]
            for k, v in attrs.items():
                row[f"attr.{k}"] = v
                if isinstance(v, str):
                    string_attr_cols.add(f"attr.{k}")

        for field, prefix in (("gen_meta", "gen"), ("exec_meta", "exec")):
            meta = row.get(field)
            if isinstance(meta, dict):
                del row[field]
                for k, v in meta.items():
                    row[f"{prefix}.{k}"] = v

        rows.append(row)

    if not rows:
        return pd.DataFrame({
            "run_id": pd.Series(dtype="object"),
            "task_index": pd.Series(dtype="int64"),
            "task_label": pd.Series(dtype="object"),
            "rep_index": pd.Series(dtype="int64"),
            "data_token": pd.Series(dtype="object"),
            "metric": pd.Series(dtype="float64"),
        })

    df = pd.DataFrame(rows)
    for col in string_attr_cols:
        df[col] = _to_category(df[col])
    return df


def _task_configurations(df: pd.DataFrame, exclude: Sequence[str] = ()) -> tuple[Any, dict[Any, str]]:
    """Number each task configuration in ``df`` and give each a distinct name.

    A configuration is a task's label, executor, args and attributes, leaving
    out the ``exclude`` columns, so the same configuration gets the same number
    in every run, whatever order its tasks ran in. Configurations that share a
    ``task_label`` are named by the args or attributes they differ in, where a
    missing attribute counts as a difference, and names that would still clash
    get a number. Returns the number of each row and the name of each number.
    """
    pd = _import_pandas()
    config_cols = [c for c in ("task_label", "executor", "args") if c in df]
    config_cols += [c for c in df.columns if c.startswith("attr.")]
    config_cols = [c for c in config_cols if c not in exclude]
    keys = df[config_cols].copy()
    for col, dtype in keys.dtypes.items():
        if dtype == object:
            keys[col] = keys[col].map(_json_key)
    configs = keys.groupby(config_cols, dropna=False, observed=True, sort=False).ngroup().to_numpy()

    by_label = {}
    for position, config in pd.Series(configs).drop_duplicates().items():
        by_label.setdefault(df["task_label"].iloc[position], []).append((position, config))

    shown = [c for c in config_cols if c == "args" or c.startswith("attr.")]
    names = {}
    for label, firsts in by_label.items():
        positions = [position for position, _ in firsts]
        differing = [c for c in shown if keys[c].iloc[positions].nunique(dropna=False) > 1]
        for position, config in firsts:
            parts = [
                f"{col.removeprefix('attr.')}={df[col].iloc[position]}"
                for col in differing
                if not _is_missing(df[col].iloc[position])
            ]
            names[config] = f"{label} [{', '.join(parts)}]" if parts else label
    return configs, _distinct(names)


class Lab:
    """Benchmark results as a dataframe, plus what the metric means.

    ``df`` has one row per result with at least ``task_label`` and ``metric``
    columns. Results read from ``impa`` also carry ``run_id``,
    ``task_index``, ``rep_index``, ``data_token``, the generator ``seed`` and
    the flattened ``attr.*``, ``gen.*`` and ``exec.*`` columns. Treat ``df``
    as read-only and use :meth:`pipe` to change it.

    ``metric_label`` names the metric on plot axes and ``lower_is_better``
    says which end of it is best. Both carry over to every Lab derived from
    this one. ``raw`` is the Lab as first built, before any transform, so a
    prepared or reduced Lab can always reach every row that was collected.
    """

    def __init__(
        self,
        df: pd.DataFrame,
        *,
        metric_label: str = "Metric",
        lower_is_better: bool = True,
    ):
        pd = _import_pandas()
        if not isinstance(df, pd.DataFrame):
            raise TypeError(f"df must be a pandas DataFrame, not {type(df).__name__}")
        missing = [c for c in REQUIRED_COLUMNS if c not in df]
        if missing:
            raise ValueError(f"df is missing columns that Lab needs: {', '.join(missing)}")

        self.df = df
        self.metric_label = metric_label
        self.lower_is_better = lower_is_better
        self.raw = self

    @classmethod
    def from_jsonl(
        cls,
        text: str,
        *,
        metric_label: str = "Metric",
        lower_is_better: bool = True,
    ) -> Self:
        """Build a Lab from the JSONL string returned by ``Impa.run``."""
        return cls(
            _records_to_frame(jsonl.loads(text)),
            metric_label=metric_label,
            lower_is_better=lower_is_better,
        )

    @classmethod
    def concat(cls, labs: Iterable[Lab]) -> Self:
        """Stack several Labs into one, such as the results of several runs.

        Every row keeps its ``run_id``, so :meth:`best_of_reps` still tells
        the runs apart. All Labs must agree on ``metric_label`` and
        ``lower_is_better``.
        """
        pd = _import_pandas()
        labs = list(labs)
        if not labs:
            raise ValueError("concat needs at least one Lab")
        if len({(lab.metric_label, lab.lower_is_better) for lab in labs}) > 1:
            raise ValueError("concat needs Labs that agree on metric_label and lower_is_better")

        df = pd.concat([lab.df for lab in labs], ignore_index=True)
        # pd.concat turns categoricals whose categories differ into plain objects.
        categorical = {
            col
            for lab in labs
            for col, dtype in lab.df.dtypes.items()
            if isinstance(dtype, pd.CategoricalDtype)
        }
        for col in categorical:
            df[col] = _to_category(df[col])

        first = labs[0]
        combined = cls(df, metric_label=first.metric_label, lower_is_better=first.lower_is_better)
        if any(lab.raw is not lab for lab in labs):
            combined.raw = cls.concat(lab.raw for lab in labs)
        return combined

    def _derive(self, df: pd.DataFrame) -> Self:
        lab = type(self)(df, metric_label=self.metric_label, lower_is_better=self.lower_is_better)
        lab.raw = self.raw
        return lab

    def pipe(self, fn: Callable[[pd.DataFrame], pd.DataFrame]) -> Self:
        """Return a Lab whose dataframe is ``fn`` applied to a copy of this one.

        Use it to cast, derive or select columns, such as parsing a
        generator's ``gen.*`` metadata. ``fn`` gets a copy, down to the lists
        and dicts stored in its cells, so changing it in place leaves this Lab
        untouched.
        """
        pd = _import_pandas()
        df = self.df.copy()
        # DataFrame.copy shares the lists and dicts that nested metadata leaves in cells.
        for col in [c for c, dtype in df.dtypes.items() if dtype == object]:
            df[col] = df[col].map(_copy_containers)
        result = fn(df)
        if not isinstance(result, pd.DataFrame):
            raise TypeError(f"pipe function must return a DataFrame, not {type(result).__name__}")
        return self._derive(result)

    def best_of_reps(self) -> Self:
        """Keep the best rep of each task on each generated input.

        Within one run, impalab replays the generator with the same seed on
        every rep, so ``data_token`` identifies the same input across reps.
        Keeping the lowest metric over reps, or the highest when
        ``lower_is_better`` is false, removes system jitter. Reps are matched
        on ``run_id``, ``task_index`` and ``data_token``, so results from
        different runs are never compared.
        """
        missing = [c for c in REP_KEYS if c not in self.df or self.df[c].isna().any()]
        if missing:
            message = (
                f"best_of_reps needs {', '.join(missing)} on every row; "
                "keep these columns in any pipe step before it"
            )
            if "run_id" in missing:
                message += (
                    ". impa adds run_id to every row from version 0.6.0. For older results, "
                    "add one with pipe, using one value per impa run"
                )
            raise ValueError(message)
        if self.df.empty:
            return self._derive(self.df.copy())

        metrics = self.df.reset_index(drop=True).groupby(list(REP_KEYS), observed=True)["metric"]
        # Positions rather than index labels, so a duplicate index can't select extra rows.
        best = metrics.idxmin() if self.lower_is_better else metrics.idxmax()
        return self._derive(self.df.iloc[best.to_numpy()])

    def summary(self, by: str | Sequence[str] | None = None) -> pd.DataFrame:
        """Return the count, mean, median, min, max and std of each task's metric.

        Each task configuration, meaning its executor, args and attributes,
        gets rows of its own, so configurations are never pooled even when
        they share an executor and args. Those are named by the args or
        attributes they differ in, such as ``zig sort [opt=O3]``, and names
        that would still clash get a number. ``by`` adds columns to group on,
        e.g. ``"gen.size"`` for one row per task and size, which then don't
        count towards a task's configuration or name. Rows are sorted by
        ``by``, then best median first.
        """
        by = [] if by is None else [by] if isinstance(by, str) else list(by)
        configs, names = _task_configurations(self.df, exclude=by)
        stats = (
            self.df.assign(_config=configs)
            .groupby([*by, "_config"], observed=True)["metric"]
            .agg(["count", "mean", "median", "min", "max", "std"])
            .reset_index()
        )
        stats.insert(len(by), "task_label", stats.pop("_config").map(names))
        return stats.sort_values(
            [*by, "median"],
            ascending=[True] * len(by) + [self.lower_is_better],
            kind="stable",
        ).reset_index(drop=True)

    def _per_x(self, x: str | None, normalized: bool) -> tuple[Lab, str]:
        """Return the Lab to plot and its metric axis label, divided by ``x`` when normalized."""
        if not normalized:
            return self, self.metric_label
        if x is None:
            raise ValueError("normalized=True needs an x column to divide by")
        divided = self._derive(self.df.assign(metric=self.df["metric"] / self.df[x]))
        return divided, f"Normalized {self.metric_label}"

    def plot_trends(
        self,
        x: str,
        title: str = "Median Performance",
        *,
        normalized: bool = False,
        xlog: bool = False,
        ylog: bool = False,
    ) -> Figure:
        """Plot the median metric of each task against ``x``.

        The lines and their names are those of ``summary(by=x)``. With
        ``normalized=True`` the metric is divided by ``x``, e.g. to get a
        per-element cost when ``x`` is the input length. Returns the plotly
        figure without showing it.
        """
        try:
            import plotly.express as px
        except ImportError as e:
            raise ImportError(
                "plotly is required for trend plots. "
                "Install it using: pip install plotly"
            ) from e
        if self.df.empty:
            raise ValueError("Lab has no rows to plot")

        lab, metric_label = self._per_x(x, normalized)
        x_label = x.title()
        fig = px.line(
            lab.summary(by=x),
            x=x,
            y="median",
            color="task_label",
            markers=True,
            title=title,
            labels={
                x: x_label,
                "median": metric_label,
                "task_label": "Task",
            },
        )
        if xlog:
            fig.update_xaxes(type="log")
        if ylog:
            fig.update_yaxes(type="log")
        fig.update_layout(
            height=600,
            hovermode="x unified",
            xaxis_title=x_label,
            yaxis_title=metric_label,
        )
        return fig

    def plot_distributions(
        self,
        x: str | None = None,
        title: str = "Performance Distributions",
        *,
        normalized: bool = False,
        ylog: bool = False,
        ax: Axes | None = None,
    ) -> Axes:
        """Box-and-strip plot of each task's metric, grouped by ``x``.

        Tasks are told apart and named as in :meth:`summary`. Draws on ``ax``
        when given, otherwise on a new figure. Returns the axes without
        showing them.
        """
        try:
            import matplotlib.pyplot as plt
            import seaborn as sns
        except ImportError as e:
            raise ImportError(
                "matplotlib and seaborn are required for plotting. "
                "Install them using: pip install matplotlib seaborn"
            ) from e
        if self.df.empty:
            raise ValueError("Lab has no rows to plot")

        lab, metric_label = self._per_x(x, normalized)
        configs, names = _task_configurations(lab.df, exclude=[x] if x else [])
        data = lab.df.assign(task_label=[names[config] for config in configs])
        if ax is None:
            _, ax = plt.subplots(figsize=(12, 10), layout="tight")
        sns.boxplot(
            data=data,
            x=x,
            y="metric",
            hue="task_label",
            dodge=True,  # the same as the stripplot, so points sit on their boxes
            fliersize=0,  # the stripplot already draws every point
            ax=ax,
        )
        sns.stripplot(
            data=data,
            x=x,
            y="metric",
            palette="dark:black",
            hue="task_label",
            dodge=True,
            jitter=0.2,
            alpha=0.3,
            size=3,
            legend=False,
            ax=ax,
        )

        ax.set_title(title, fontsize=16)
        ax.set_xlabel((x or "Task").title(), fontsize=12)
        ax.set_ylabel(metric_label, fontsize=12)
        plt.setp(ax.get_xticklabels(), rotation=45, ha="right")
        ax.grid(axis="y", linestyle="--", alpha=0.7)
        ax.legend(title="Task", fontsize=10)
        if ylog:
            ax.set_yscale("log")
        return ax
