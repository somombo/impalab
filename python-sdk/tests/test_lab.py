from unittest import mock

import pytest

from impalab_py import Lab, jsonl

pd = pytest.importorskip("pandas")

RUN_ID = "01a1fd89-6d24-74e3-867e-6e246e06029c"
OTHER_RUN_ID = "01a1fd8a-0b3c-7d4e-9f50-61728394a5b6"
TASKS = ["linear_search", "binary_search"]
SIZES = {"case_1": 10, "case_2": 20, "case_3": 20}
# Two reps of each task on each input. case_2 and case_3 are different inputs of
# the same size, so reducing by generator parameters alone would pool them.
REP_METRICS = {
    (0, "case_1"): [450, 440],
    (0, "case_2"): [900, 910],
    (0, "case_3"): [800, 820],
    (1, "case_1"): [35, 30],
    (1, "case_2"): [60, 50],
    (1, "case_3"): [45, 40],
}
REP_COUNT = sum(len(metrics) for metrics in REP_METRICS.values())

RESULTS = jsonl.dumps([
    {
        "run_id": RUN_ID,
        "seed": "42",
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

MOCK_RESULTS_JSONL = """
{"run_id":"01a1fd89-6d24-74e3-867e-6e246e06029c","seed":"42","task_index":0,"executor":"zig-executors","args":["linear_search"],"rep_index":0,"attributes":{"cpu":"x86_64"},"data_token":"case_1","metric":450,"gen_meta":{"size":100},"exec_meta":{"iters":10}}
{"run_id":"01a1fd89-6d24-74e3-867e-6e246e06029c","seed":"42","task_index":1,"executor":"zig-executors","args":["binary_search"],"rep_index":0,"attributes":{"cpu":"x86_64"},"data_token":"case_1","metric":200,"gen_meta":{"size":100}}
{"run_id":"01a1fd89-6d24-74e3-867e-6e246e06029c","seed":"42","task_index":0,"executor":"zig-executors","args":["linear_search"],"rep_index":1,"attributes":{"cpu":"x86_64"},"data_token":"case_2","metric":460,"gen_meta":{"size":100},"exec_meta":{"iters":12}}
"""


def row(task_index, args, rep_index, data_token, metric, run_id=RUN_ID, **fields):
    return {
        "run_id": run_id,
        "task_index": task_index,
        "executor": "zig",
        "args": args,
        "rep_index": rep_index,
        "data_token": data_token,
        "metric": metric,
        **fields,
    }


def lab_of(*rows, **settings):
    return Lab.from_jsonl(jsonl.dumps(list(rows)), **settings)


def metrics_by_task_and_input(df):
    return {
        (task_index, data_token): metric
        for task_index, data_token, metric in zip(df["task_index"], df["data_token"], df["metric"])
    }


@pytest.fixture
def plt():
    matplotlib = pytest.importorskip("matplotlib")
    pytest.importorskip("seaborn")
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    yield plt
    plt.close("all")


def test_from_jsonl_flattens_attributes_and_metadata():
    df = Lab.from_jsonl(MOCK_RESULTS_JSONL).df

    assert len(df) == 3
    assert list(df["task_label"]) == [
        "zig-executors linear_search",
        "zig-executors binary_search",
        "zig-executors linear_search",
    ]
    assert (df["run_id"].iloc[0], df["seed"].iloc[0]) == (RUN_ID, "42")
    assert df["attr.cpu"].dtype == "category"
    assert df["gen.size"].iloc[0] == 100
    assert df["exec.iters"].iloc[0] == 10
    assert pd.isna(df["exec.iters"].iloc[1])  # binary_search has no exec_meta


def test_from_jsonl_stores_attributes_with_any_string_value_as_categories():
    df = lab_of(
        row(0, ["sort"], 0, "case_1", 1, attributes={"string_attr": "hello", "numeric_attr": 42, "mixed_attr": "world"}),
        row(0, ["sort"], 1, "case_1", 2, attributes={"string_attr": "goodbye", "numeric_attr": 100, "mixed_attr": 200}),
    ).df

    assert df["attr.string_attr"].dtype == "category"
    assert df["attr.mixed_attr"].dtype == "category"
    assert df["attr.numeric_attr"].dtype != "category"


def test_from_jsonl_keeps_attributes_holding_lists_as_objects():
    df = lab_of(
        row(0, ["sort"], 0, "case_1", 1, attributes={"shape": "flat"}),
        row(1, ["pdq"], 0, "case_1", 2, attributes={"shape": [2, 3]}),
    ).df

    assert df["attr.shape"].dtype == object
    assert df["attr.shape"].tolist() == ["flat", [2, 3]]


def test_from_jsonl_requires_a_jsonl_string():
    with pytest.raises(TypeError):
        Lab.from_jsonl(jsonl.loads(MOCK_RESULTS_JSONL))


def test_lab_rejects_frames_without_the_columns_it_needs():
    with pytest.raises(ValueError, match="task_label"):
        Lab(pd.DataFrame({"metric": [1.0]}))
    with pytest.raises(TypeError, match="DataFrame"):
        Lab([{"task_label": "zig sort", "metric": 1.0}])


def test_summary_names_configurations_that_share_an_executor_and_args_by_their_attributes():
    lab = lab_of(
        row(0, ["sort"], 0, "case_1", 100, attributes={"opt": "O2", "cpu": "x86_64"}),
        row(1, ["sort"], 0, "case_1", 50, attributes={"opt": "O3", "cpu": "x86_64"}),
        row(2, ["sort", "#1"], 0, "case_1", 70, attributes={"opt": "O2", "cpu": "x86_64"}),
    )

    assert list(lab.summary()["task_label"]) == ["zig sort [opt=O3]", "zig sort #1", "zig sort [opt=O2]"]
    assert set(lab.df["task_label"]) == {"zig sort", "zig sort #1"}


def test_summary_never_pools_configurations_from_different_runs():
    o2 = lab_of(row(0, ["sort"], 0, "case_1", 100, attributes={"opt": "O2"}))
    o3 = lab_of(row(0, ["sort"], 0, "case_1", 50, run_id=OTHER_RUN_ID, attributes={"opt": "O3"}))

    stats = Lab.concat([o2, o3]).summary()

    assert list(zip(stats["task_label"], stats["median"])) == [
        ("zig sort [opt=O3]", 50.0),
        ("zig sort [opt=O2]", 100.0),
    ]


def test_summary_pools_each_configuration_across_runs_whatever_order_its_tasks_ran_in():
    first = lab_of(
        row(0, ["sort"], 0, "case_1", 100, attributes={"opt": "O2"}),
        row(1, ["sort"], 0, "case_1", 50, attributes={"opt": "O3"}),
    )
    second = lab_of(
        row(0, ["sort"], 0, "case_1", 52, run_id=OTHER_RUN_ID, attributes={"opt": "O3"}),
        row(1, ["sort"], 0, "case_1", 102, run_id=OTHER_RUN_ID, attributes={"opt": "O2"}),
    )

    stats = Lab.concat([first, second]).summary()

    assert list(zip(stats["task_label"], stats["count"], stats["median"])) == [
        ("zig sort [opt=O3]", 2, 51.0),
        ("zig sort [opt=O2]", 2, 101.0),
    ]


def test_summary_leaves_the_columns_it_groups_by_out_of_task_names():
    lab = lab_of(*(
        row(task_index, ["sort"], 0, "case_1", 10 * threads, attributes={"threads": threads})
        for task_index, threads in enumerate([1, 2, 4])
    ))

    stats = lab.summary(by="attr.threads")

    assert list(zip(stats["attr.threads"], stats["task_label"])) == [(1, "zig sort"), (2, "zig sort"), (4, "zig sort")]


def test_tasks_with_identical_configurations_pool_in_summaries_but_not_in_best_of_reps():
    lab = lab_of(row(0, ["sort"], 0, "case_1", 1), row(1, ["sort"], 0, "case_1", 2))

    assert lab.summary()["count"].tolist() == [2]
    assert len(lab.best_of_reps().df) == 2


def test_best_of_reps_keeps_lowest_metric_of_each_task_on_each_input():
    best = Lab.from_jsonl(RESULTS).best_of_reps().df

    assert len(best) == len(REP_METRICS)
    assert metrics_by_task_and_input(best) == {key: min(metrics) for key, metrics in REP_METRICS.items()}


def test_best_of_reps_keeps_highest_metric_when_higher_is_better():
    best = Lab.from_jsonl(RESULTS, lower_is_better=False).best_of_reps().df

    assert len(best) == len(REP_METRICS)
    assert metrics_by_task_and_input(best) == {key: max(metrics) for key, metrics in REP_METRICS.items()}


def test_best_of_reps_never_compares_rows_from_different_runs():
    # Both runs reuse the token case_1, so only the run_id tells their inputs apart.
    small = lab_of(*(row(0, ["sort"], rep, "case_1", m) for rep, m in enumerate([5, 6])))
    large = lab_of(*(row(0, ["sort"], rep, "case_1", m, run_id=OTHER_RUN_ID) for rep, m in enumerate([500, 600])))

    best = Lab.concat([small, large]).best_of_reps().df

    assert dict(zip(best["run_id"], best["metric"])) == {RUN_ID: 5, OTHER_RUN_ID: 500}


def before_run_id(*results):
    return [{key: value for key, value in result.items() if key != "run_id"} for result in results]


def test_best_of_reps_refuses_rows_without_a_run_id():
    legacy = lab_of(*before_run_id(row(0, ["sort"], 0, "case_1", 5)))
    mixed = Lab.concat([legacy, lab_of(row(0, ["sort"], 0, "case_1", 6))])

    for lab in (legacy, mixed):
        with pytest.raises(ValueError, match="run_id.*0.6.0"):
            lab.best_of_reps()


def test_best_of_reps_accepts_a_run_id_added_to_older_results():
    legacy = lab_of(*before_run_id(row(0, ["sort"], 0, "case_1", 5), row(0, ["sort"], 1, "case_1", 6)))

    best = legacy.pipe(lambda df: df.assign(run_id="experiment 1")).best_of_reps().df

    assert best["metric"].tolist() == [5]


def test_best_of_reps_ignores_duplicate_index_labels():
    doubled = Lab.from_jsonl(RESULTS).pipe(lambda df: pd.concat([df, df]))

    assert len(doubled.best_of_reps().df) == len(REP_METRICS)


def test_best_of_reps_needs_the_columns_that_identify_an_input():
    lab = Lab.from_jsonl(RESULTS).pipe(lambda df: df.drop(columns="data_token"))

    with pytest.raises(ValueError, match="data_token"):
        lab.best_of_reps()


def test_pipe_hands_fn_a_copy_and_keeps_settings_and_raw():
    raw = Lab.from_jsonl(RESULTS, metric_label="Duration (ns)", lower_is_better=False)

    def prepare(df):
        df["size_squared"] = df["gen.size"] ** 2
        return df

    prepared = raw.pipe(prepare)
    reduced = prepared.best_of_reps()

    assert "size_squared" in prepared.df
    assert "size_squared" not in raw.df
    assert (reduced.metric_label, reduced.lower_is_better) == ("Duration (ns)", False)
    assert reduced.raw is raw
    assert raw.raw is raw


def test_pipe_copies_lists_and_dicts_stored_in_cells():
    raw = lab_of(row(0, ["sort"], 0, "case_1", 1, exec_meta={"details": [1], "phases": {"warm": [2]}}))

    def mutate(df):
        df["args"].iloc[0].append("extra")
        df["exec.details"].iloc[0].append(99)
        df["exec.phases"].iloc[0]["warm"].append(99)
        return df

    raw.pipe(mutate)

    assert raw.df["args"].iloc[0] == ["sort"]
    assert raw.df["exec.details"].iloc[0] == [1]
    assert raw.df["exec.phases"].iloc[0] == {"warm": [2]}


def test_pipe_rejects_functions_that_do_not_return_a_dataframe():
    def forgets_to_return(df):
        df["size_squared"] = df["gen.size"] ** 2

    with pytest.raises(TypeError, match="NoneType"):
        Lab.from_jsonl(RESULTS).pipe(forgets_to_return)


def test_concat_stacks_labs_and_combines_their_raw_rows():
    reduced = Lab.from_jsonl(RESULTS).best_of_reps()
    raw = Lab.from_jsonl(RESULTS.replace(RUN_ID, OTHER_RUN_ID))

    combined = Lab.concat([reduced, raw])

    assert combined.df["run_id"].value_counts().to_dict() == {RUN_ID: len(REP_METRICS), OTHER_RUN_ID: REP_COUNT}
    assert combined.raw.df["run_id"].value_counts().to_dict() == {RUN_ID: REP_COUNT, OTHER_RUN_ID: REP_COUNT}


def test_concat_keeps_categorical_columns_categorical():
    x86 = lab_of(row(0, ["sort"], 0, "case_1", 1, attributes={"cpu": "x86_64"}))
    arm = lab_of(row(0, ["sort"], 0, "case_1", 2, run_id=OTHER_RUN_ID, attributes={"cpu": "arm64"}))
    also_x86 = lab_of(row(0, ["sort"], 0, "case_1", 3, run_id=OTHER_RUN_ID, attributes={"cpu": "x86_64"}))

    for labs in ([x86, arm], [x86, also_x86], [x86]):
        assert Lab.concat(labs).df["attr.cpu"].dtype == "category"


def test_concat_keeps_attributes_holding_lists_as_objects():
    text = lab_of(row(0, ["sort"], 0, "case_1", 1, attributes={"shape": "flat"}))
    listed = lab_of(row(0, ["sort"], 0, "case_1", 2, run_id=OTHER_RUN_ID, attributes={"shape": [2, 3]}))

    assert Lab.concat([text, listed]).df["attr.shape"].tolist() == ["flat", [2, 3]]


def test_concat_rejects_labs_that_disagree_on_the_metric():
    with pytest.raises(ValueError, match="metric_label"):
        Lab.concat([Lab.from_jsonl(RESULTS), Lab.from_jsonl(RESULTS, metric_label="Duration (ns)")])
    with pytest.raises(ValueError, match="at least one"):
        Lab.concat([])


def test_summary_ranks_tasks_by_median_within_each_group():
    stats = Lab.from_jsonl(RESULTS).best_of_reps().summary(by="gen.size")

    assert list(zip(stats["gen.size"], stats["task_label"], stats["median"])) == [
        (10, "zig-executors binary_search", 30.0),
        (10, "zig-executors linear_search", 440.0),
        (20, "zig-executors binary_search", 45.0),
        (20, "zig-executors linear_search", 850.0),
    ]


def test_summary_puts_the_best_task_first_in_either_direction():
    lower = Lab.from_jsonl(RESULTS).summary()
    higher = Lab.from_jsonl(RESULTS, lower_is_better=False).summary()

    assert list(lower["task_label"]) == ["zig-executors binary_search", "zig-executors linear_search"]
    assert list(higher["task_label"]) == ["zig-executors linear_search", "zig-executors binary_search"]
    assert lower["count"].tolist() == [REP_COUNT // 2, REP_COUNT // 2]


def test_an_empty_lab_reduces_and_summarizes_to_empty_frames():
    empty = Lab.from_jsonl("")

    assert empty.best_of_reps().df.empty
    assert empty.summary().empty


def test_plot_trends_draws_summary_medians_without_showing():
    go = pytest.importorskip("plotly.graph_objects")
    lab = Lab.from_jsonl(RESULTS).best_of_reps()

    with mock.patch.object(go.Figure, "show") as show:
        fig = lab.plot_trends("gen.size", normalized=True)

    show.assert_not_called()
    assert fig.layout.yaxis.title.text == "Normalized Metric"
    # Median of each task's best metric per element, e.g. (900 / 20 + 800 / 20) / 2 at size 20.
    assert {trace.name: dict(zip(trace.x, trace.y)) for trace in fig.data} == {
        "zig-executors linear_search": {10: 44.0, 20: 42.5},
        "zig-executors binary_search": {10: 3.0, 20: 2.25},
    }


def test_plot_distributions_draws_every_row_on_given_axes(plt):
    lab = Lab.from_jsonl(RESULTS).best_of_reps()
    _, ax = plt.subplots()

    with mock.patch.object(plt, "show") as show:
        returned = lab.plot_distributions("gen.size", ylog=True, ax=ax)

    show.assert_not_called()
    assert returned is ax
    assert ax.get_xlabel() == "Gen.Size"
    assert ax.get_ylabel() == "Metric"
    assert ax.get_yscale() == "log"
    assert sum(len(points.get_offsets()) for points in ax.collections) == len(REP_METRICS)


def test_plot_distributions_names_tasks_as_summary_does(plt):
    lab = lab_of(
        row(0, ["sort"], 0, "case_1", 100, gen_meta={"size": 10}, attributes={"opt": "O2"}),
        row(1, ["sort"], 0, "case_1", 50, gen_meta={"size": 10}, attributes={"opt": "O3"}),
    )

    ax = lab.plot_distributions("gen.size")

    assert [text.get_text() for text in ax.get_legend().get_texts()] == ["zig sort [opt=O2]", "zig sort [opt=O3]"]


@pytest.mark.parametrize("tasks, names", [
    pytest.param(
        [(["sort"], None), (["sort"], {"opt": "O3"})],
        ["zig sort", "zig sort [opt=O3]"],
        id="one task lacks an attribute",
    ),
    pytest.param(
        [(["a b"], None), (["a", "b"], None)],
        ["zig a b [args=['a b']]", "zig a b [args=['a', 'b']]"],
        id="args join into the same label",
    ),
    pytest.param(
        [(["sort"], {"shape": [2, 3]}), (["sort"], {"shape": "[2, 3]"})],
        ["zig sort [shape=[2, 3]] #1", "zig sort [shape=[2, 3]] #2"],
        id="a list and a string that reads the same",
    ),
])
def test_configurations_that_could_share_a_name_are_summarized_and_plotted_apart(plt, tasks, names):
    pytest.importorskip("plotly")
    lab = lab_of(*(
        row(task_index, args, 0, "case_1", metric, gen_meta={"size": 10}, **({"attributes": attributes} if attributes else {}))
        for task_index, ((args, attributes), metric) in enumerate(zip(tasks, [100, 10]))
    ))

    stats = lab.summary()
    traces = lab.plot_trends("gen.size").data
    legend = lab.plot_distributions("gen.size").get_legend().get_texts()

    assert dict(zip(stats["task_label"], stats["median"])) == dict(zip(names, [100.0, 10.0]))
    assert sorted(trace.name for trace in traces) == sorted(names)
    assert sorted(text.get_text() for text in legend) == sorted(names)


def test_plot_distributions_draws_each_tasks_points_on_its_box(plt):
    # Each size has samples from one task only, so seaborn would not dodge the boxes on its own.
    df = pd.DataFrame({
        "gen.size": [10] * 101 + [20] * 101,
        "metric": list(range(101)) + list(range(1000, 1101)),
        "task_label": ["zig sort"] * 101 + ["zig pdq"] * 101,
    })

    ax = Lab(df).plot_distributions("gen.size")

    medians = sorted(sum(line.get_xdata()) / 2 for line in ax.lines if list(line.get_ydata()) in ([50, 50], [1050, 1050]))
    points = sorted(points.get_offsets()[:, 0].mean() for points in ax.collections if len(points.get_offsets()))
    assert medians == pytest.approx(points, abs=0.02)


def test_normalized_plots_require_an_x_column(plt):
    with pytest.raises(ValueError, match="x column"):
        Lab.from_jsonl(RESULTS).plot_distributions(normalized=True)


def test_plots_reject_an_empty_lab(plt):
    pytest.importorskip("plotly")
    empty = Lab.from_jsonl("")

    with pytest.raises(ValueError, match="no rows"):
        empty.plot_distributions()
    with pytest.raises(ValueError, match="no rows"):
        empty.plot_trends("gen.size")


def runs_sharing_a_token():
    # Both runs reuse the token case_1, as generators with simple tokens do.
    small = jsonl.dumps([row(0, ["sort"], rep, "case_1", m) for rep, m in enumerate([5, 6])])
    large = jsonl.dumps([row(0, ["sort"], rep, "case_1", m, run_id=OTHER_RUN_ID) for rep, m in enumerate([500, 600])])
    return small, large


def test_read_jsonl_gives_back_the_rows_that_were_saved(tmp_path):
    path = tmp_path / "results.jsonl.gz"
    jsonl.write(path, RESULTS)

    lab = Lab.read_jsonl(path, metric_label="Duration (ns)")

    pd.testing.assert_frame_equal(lab.df, Lab.from_jsonl(RESULTS).df)
    assert lab.metric_label == "Duration (ns)"


def test_read_jsonl_reads_a_glob_of_files_and_keeps_their_runs_apart(tmp_path):
    small, large = runs_sharing_a_token()
    jsonl.write(tmp_path / "study-small.jsonl", small)
    jsonl.write(tmp_path / "study-large.jsonl.gz", large)

    best = Lab.read_jsonl(str(tmp_path / "study-*")).best_of_reps().df

    assert dict(zip(best["run_id"], best["metric"])) == {RUN_ID: 5, OTHER_RUN_ID: 500}


def test_read_jsonl_keeps_runs_apart_within_one_file(tmp_path):
    path = tmp_path / "study.jsonl"
    jsonl.write(path, jsonl.join(list(runs_sharing_a_token())))

    best = Lab.read_jsonl(path).best_of_reps().df

    assert dict(zip(best["run_id"], best["metric"])) == {RUN_ID: 5, OTHER_RUN_ID: 500}


def test_read_jsonl_accepts_a_list_of_paths(tmp_path):
    paths = [tmp_path / "a.jsonl", tmp_path / "b.jsonl"]
    jsonl.write(paths[0], RESULTS)
    jsonl.write(paths[1], RESULTS.replace(RUN_ID, OTHER_RUN_ID))

    lab = Lab.read_jsonl(paths)

    assert lab.df["run_id"].value_counts().to_dict() == {RUN_ID: REP_COUNT, OTHER_RUN_ID: REP_COUNT}


def test_read_jsonl_prefers_an_existing_file_over_a_glob_match(tmp_path):
    jsonl.write(tmp_path / "results[1].jsonl", jsonl.dumps([row(0, ["literal"], 0, "case_1", 1)]))
    jsonl.write(tmp_path / "results1.jsonl", jsonl.dumps([row(0, ["globbed"], 0, "case_1", 2)]))

    for path in (tmp_path / "results[1].jsonl", str(tmp_path / "results[1].jsonl")):
        assert list(Lab.read_jsonl(path).df["task_label"]) == ["zig literal"]


def test_read_jsonl_raises_when_a_pattern_matches_nothing(tmp_path):
    with pytest.raises(FileNotFoundError, match="no files match"):
        Lab.read_jsonl(str(tmp_path / "missing-*.jsonl"))
