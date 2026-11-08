from unittest import mock

import pytest

from impalab_py import Explorer, Lab, best_of_reps, jsonl

pd = pytest.importorskip("pandas")

TASKS = ["linear_search", "binary_search"]
SIZES = {"run_1": 10, "run_2": 20, "run_3": 20}
# Two reps of each task on each input. run_2 and run_3 are different inputs of
# the same size, so reducing by generator parameters alone would pool them.
REP_METRICS = {
    (0, "run_1"): [450, 440],
    (0, "run_2"): [900, 910],
    (0, "run_3"): [800, 820],
    (1, "run_1"): [35, 30],
    (1, "run_2"): [60, 50],
    (1, "run_3"): [45, 40],
}

RESULTS = jsonl.dumps([
    {
        "task_index": task_index,
        "executor": "zig-executors",
        "args": [TASKS[task_index]],
        "rep_index": rep_index,
        "data_token": data_token,
        "gen_meta": {"size": SIZES[data_token]},
        "metric": metric,
    }
    for (task_index, data_token), metrics in REP_METRICS.items()
    for rep_index, metric in enumerate(metrics)
])


@pytest.fixture
def plt():
    matplotlib = pytest.importorskip("matplotlib")
    pytest.importorskip("seaborn")
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    yield plt
    plt.close("all")


def metrics_by_task_and_input(df):
    return {
        (task_index, data_token): metric
        for task_index, data_token, metric in zip(df["task_index"], df["data_token"], df["metric"])
    }


def test_best_of_reps_keeps_lowest_metric_of_each_task_on_each_input():
    best = best_of_reps(Lab(RESULTS).to_dataframe())

    assert len(best) == len(REP_METRICS)
    assert metrics_by_task_and_input(best) == {key: min(metrics) for key, metrics in REP_METRICS.items()}


def test_best_of_reps_keeps_highest_metric_when_higher_is_better():
    best = best_of_reps(Lab(RESULTS).to_dataframe(), lower_is_better=False)

    assert len(best) == len(REP_METRICS)
    assert metrics_by_task_and_input(best) == {key: max(metrics) for key, metrics in REP_METRICS.items()}


def test_best_of_reps_copies_empty_frames():
    empty = pd.DataFrame()
    reduced = best_of_reps(empty)

    assert reduced.empty
    assert reduced is not empty


def test_from_results_prepares_clean_data_without_touching_raw_data():
    def prepare(df):
        df["size_squared"] = df["gen.size"] ** 2
        return df

    explorer = Explorer.from_results(RESULTS, prepare=prepare, metric_label="Duration (ns)")

    pd.testing.assert_frame_equal(explorer.get_raw_data(), Lab(RESULTS).to_dataframe())
    assert "size_squared" in explorer.get_clean_data().columns
    assert len(explorer.get_clean_data()) == len(REP_METRICS)
    assert explorer.metric_label == "Duration (ns)"


def test_plot_trends_interactive_returns_medians_without_showing():
    go = pytest.importorskip("plotly.graph_objects")
    explorer = Explorer.from_results(RESULTS)

    with mock.patch.object(go.Figure, "show") as show:
        fig = explorer.plot_trends_interactive("gen.size", normalized=True)

    show.assert_not_called()
    assert fig.layout.yaxis.title.text == "Normalized Metric"
    # Median of each task's best metric per element, e.g. (900 / 20 + 800 / 20) / 2 at size 20.
    assert {trace.name: dict(zip(trace.x, trace.y)) for trace in fig.data} == {
        "zig-executors linear_search": {10: 44.0, 20: 42.5},
        "zig-executors binary_search": {10: 3.0, 20: 2.25},
    }


def test_plot_distributions_draws_every_clean_row_on_given_axes(plt):
    explorer = Explorer.from_results(RESULTS)
    _, ax = plt.subplots()

    with mock.patch.object(plt, "show") as show:
        returned = explorer.plot_distributions("gen.size", ylog=True, ax=ax)

    show.assert_not_called()
    assert returned is ax
    assert ax.get_xlabel() == "Gen.Size"
    assert ax.get_ylabel() == "Metric"
    assert ax.get_yscale() == "log"
    assert sum(len(points.get_offsets()) for points in ax.collections) == len(REP_METRICS)


def test_normalized_plots_require_a_varying_parameter(plt):
    with pytest.raises(ValueError, match="varying_param_name"):
        Explorer.from_results(RESULTS).plot_distributions(normalized=True)


def test_plots_return_none_without_results(plt, capsys):
    pytest.importorskip("plotly")
    explorer = Explorer.from_results("")

    assert explorer.plot_distributions() is None
    assert explorer.plot_trends_interactive("gen.size") is None
    assert "Dataframe is empty" in capsys.readouterr().out


def test_plot_distributions_draws_each_tasks_points_on_its_box(plt):
    # Each size has samples from one task only, so seaborn would not dodge the boxes on its own.
    df = pd.DataFrame({
        "gen.size": [10] * 101 + [20] * 101,
        "metric": list(range(101)) + list(range(1000, 1101)),
        "task_label": ["zig sort"] * 101 + ["zig pdq"] * 101,
    })

    ax = Explorer(df).plot_distributions("gen.size")

    medians = sorted(sum(line.get_xdata()) / 2 for line in ax.lines if list(line.get_ydata()) in ([50, 50], [1050, 1050]))
    points = sorted(points.get_offsets()[:, 0].mean() for points in ax.collections if len(points.get_offsets()))
    assert medians == pytest.approx(points, abs=0.02)
