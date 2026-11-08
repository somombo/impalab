"""Explore impalab results as pandas dataframes.

Nothing here depends on a particular generator's ``gen.*`` schema.
Generator-specific parsing is injected through the ``prepare`` hook of
:meth:`Explorer.from_results`.
"""

from __future__ import annotations

from collections.abc import Callable, Sequence
from typing import TYPE_CHECKING, Self

from .lab import Lab, LabFromResults

if TYPE_CHECKING:
    import pandas as pd
    from matplotlib.axes import Axes
    from plotly.graph_objects import Figure


def best_of_reps(
    df: pd.DataFrame,
    keys: Sequence[str] = ("task_index", "data_token"),
    lower_is_better: bool = True,
) -> pd.DataFrame:
    """Keep the best rep of each task on each generated input.

    impalab replays the generator with the same seed on every rep, so
    ``data_token`` identifies the same input across reps. Keeping the lowest
    metric over reps, or the highest when ``lower_is_better`` is false,
    removes system jitter.
    """
    if df.empty:
        return df.copy()  # never hand back the caller's frame itself
    metrics = df.groupby(list(keys), observed=True)["metric"]
    return df.loc[metrics.idxmin() if lower_is_better else metrics.idxmax()]


def _display(obj) -> None:
    try:
        from IPython.display import display
    except ImportError:
        print(obj)
    else:
        display(obj)


class Explorer:
    """Compare per-task metrics across a varying generator parameter.

    ``raw_df`` holds the results as collected, one row per rep, and
    ``clean_df`` holds the rows that the analysis and plots use.
    """

    def __init__(
        self,
        clean_df: pd.DataFrame,
        raw_df: pd.DataFrame | None = None,
        lab: Lab | None = None,
        metric_label: str = "Metric",
    ):
        self.clean_df = clean_df
        self.raw_df = raw_df
        self.lab = lab
        self.metric_label = metric_label

    @classmethod
    def from_results(
        cls,
        results: str,
        prepare: Callable[[pd.DataFrame], pd.DataFrame] | None = None,
        lower_is_better: bool = True,
        **kwargs,
    ) -> Self:
        """Build an explorer from the JSONL string returned by ``Impa.run``.

        ``raw_df`` is ``Lab.to_dataframe()`` unchanged. :func:`best_of_reps`
        reduces it to one row per task and input, then ``prepare`` shapes those
        rows into the clean dataframe, e.g. by casting, deriving or selecting
        ``gen.*`` columns, so analysis choices never alter ``raw_df``. Other
        keyword arguments go to the constructor.
        """
        lab = LabFromResults(results)
        raw_df = lab.to_dataframe()
        clean_df = best_of_reps(raw_df, lower_is_better=lower_is_better)
        if prepare is not None:
            clean_df = prepare(clean_df)
        return cls(clean_df, raw_df=raw_df, lab=lab, **kwargs)

    def get_raw_data(self) -> pd.DataFrame | None:
        """Return the results as collected, one row per rep."""
        return self.raw_df

    def get_clean_data(self) -> pd.DataFrame:
        """Return the rows that the analysis and plots use."""
        return self.clean_df

    def _metric_frame(self, varying_param_name: str | None, normalized: bool) -> tuple[pd.DataFrame, str, str]:
        """Return the dataframe to plot with its metric column and axis label."""
        df = self.clean_df
        if not normalized:
            return df, "metric", self.metric_label
        if varying_param_name is None:
            raise ValueError("normalized=True requires a varying_param_name to divide by")
        df = df.assign(normalized_metric=df["metric"] / df[varying_param_name])
        return df, "normalized_metric", f"Normalized {self.metric_label}"

    def plot_trends_interactive(
        self,
        varying_param_name: str,
        title: str = "Median Performance",
        normalized: bool = False,
        xlog: bool = False,
        ylog: bool = False,
        show_table: bool = False,
    ) -> Figure | None:
        """Plot the median metric of each task against ``varying_param_name``.

        With ``normalized=True`` the metric is divided by the varying parameter,
        e.g. to get a per-element cost when varying the input length. Returns
        the plotly figure without showing it, or ``None`` when there is no data.
        """
        try:
            import plotly.express as px
        except ImportError as e:
            raise ImportError(
                "plotly is required for interactive plots. "
                "Install it using: pip install plotly"
            ) from e

        if self.clean_df.empty:
            print("Dataframe is empty. Skipping visualization.")
            return None

        df, metric, metric_label = self._metric_frame(varying_param_name, normalized)
        df_summary = df.groupby([varying_param_name, "task_label"], observed=True)[metric].median().reset_index()

        if show_table:
            print("Median metrics:")
            _display(df_summary)

        x_label = varying_param_name.title()
        fig = px.line(
            df_summary,
            x=varying_param_name,
            y=metric,
            color="task_label",
            markers=True,
            title=title,
            labels={
                varying_param_name: x_label,
                metric: metric_label,
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
        varying_param_name: str | None = None,
        title: str = "Performance Distributions",
        normalized: bool = False,
        ylog: bool = False,
        ax: Axes | None = None,
    ) -> Axes | None:
        """Box-and-strip plot of each task's metric, grouped by ``varying_param_name``.

        Draws on ``ax`` when given, otherwise on a new figure. Returns the axes
        without showing them, or ``None`` when there is no data.
        """
        try:
            import matplotlib.pyplot as plt
            import seaborn as sns
        except ImportError as e:
            raise ImportError(
                "matplotlib and seaborn are required for plotting. "
                "Install them using: pip install matplotlib seaborn"
            ) from e

        if self.clean_df.empty:
            print("Dataframe is empty. Skipping visualization.")
            return None

        df, metric, metric_label = self._metric_frame(varying_param_name, normalized)

        if ax is None:
            _, ax = plt.subplots(figsize=(12, 10), layout="tight")
        sns.boxplot(
            data=df,
            x=varying_param_name,
            y=metric,
            hue="task_label",
            dodge=True,  # the same as the stripplot, so points sit on their boxes
            fliersize=0,  # the stripplot already draws every point
            ax=ax,
        )
        sns.stripplot(
            data=df,
            x=varying_param_name,
            y=metric,
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
        ax.set_xlabel((varying_param_name or "Task").title(), fontsize=12)
        ax.set_ylabel(metric_label, fontsize=12)
        plt.setp(ax.get_xticklabels(), rotation=45, ha="right")
        ax.grid(axis="y", linestyle="--", alpha=0.7)
        ax.legend(title="Task", fontsize=10)
        if ylog:
            ax.set_yscale("log")
        return ax
