import os
import json
import sys
from unittest import mock
import pytest
from impalab_py import Impa, Lab, LabFromResults

MOCK_RESULTS_JSONL = """
{"task_index":0,"executor":"zig-executors","args":["linear_search"],"rep_index":0,"attributes":{"cpu":"x86_64"},"data_token":"run_1","metric":450,"gen_meta":{"size":100},"exec_meta":{"iters":10}}
{"task_index":1,"executor":"zig-executors","args":["binary_search"],"rep_index":0,"attributes":{"cpu":"x86_64"},"data_token":"run_1","metric":200,"gen_meta":{"size":100}}
{"task_index":0,"executor":"zig-executors","args":["linear_search"],"rep_index":1,"attributes":{"cpu":"x86_64"},"data_token":"run_2","metric":460,"gen_meta":{"size":100},"exec_meta":{"iters":12}}
"""

MOCK_RESULTS_LIST = [
    {
        "task_index": 0,
        "executor": "zig-executors",
        "args": ["linear_search"],
        "rep_index": 0,
        "attributes": {"cpu": "x86_64"},
        "data_token": "run_1",
        "metric": 450,
        "gen_meta": {"size": 100},
        "exec_meta": {"iters": 10}
    },
    {
        "task_index": 1,
        "executor": "zig-executors",
        "args": ["binary_search"],
        "rep_index": 0,
        "attributes": {"cpu": "x86_64"},
        "data_token": "run_1",
        "metric": 200,
        "gen_meta": {"size": 100}
    },
    {
        "task_index": 0,
        "executor": "zig-executors",
        "args": ["linear_search"],
        "rep_index": 1,
        "attributes": {"cpu": "x86_64"},
        "data_token": "run_2",
        "metric": 460,
        "gen_meta": {"size": 100},
        "exec_meta": {"iters": 12}
    }
]

@pytest.fixture
def temp_impa(tmp_path):
    root_dir = tmp_path / "project"
    root_dir.mkdir()
    manifest = "test_manifest.json"
    
    # Create fake executable file to avoid download logic by default
    impa_bin_dir = root_dir / "target" / "debug"
    impa_bin_dir.mkdir(parents=True)
    impa_bin = impa_bin_dir / ("impa.exe" if os.name == "nt" else "impa")
    impa_bin.touch()
    
    return Impa(
        root_dir=str(root_dir),
        manifest_filename=manifest,
        impa_path=str(impa_bin_dir)
    )

def test_impa_init_resolves_paths(temp_impa, tmp_path):
    assert os.path.isabs(temp_impa.root_dir)
    assert temp_impa.manifest_filename == "test_manifest.json"
    
    exe_name = "impa.exe" if os.name == "nt" else "impa"
    expected_exe = tmp_path / "project" / "target" / "debug" / exe_name
    assert os.path.abspath(temp_impa.impa_executable) == os.path.abspath(expected_exe)

@mock.patch("urllib.request.urlopen")
def test_ensure_executable_downloads_when_missing(mock_urlopen, tmp_path):
    mock_response = mock.MagicMock()
    mock_response.__enter__.return_value = mock_response
    mock_response.read.side_effect = [b"binary_content", b""]
    mock_urlopen.return_value = mock_response
    
    impa = Impa(
        root_dir=str(tmp_path),
        manifest_filename="manifest.json",
        impa_path=str(tmp_path / "missing_impa")
    )
    
    resolved = impa._ensure_executable()
    
    mock_urlopen.assert_called_once()
    
    assert os.path.isfile(impa.impa_executable)
    with open(impa.impa_executable, "rb") as f:
        assert f.read() == b"binary_content"
        
    if os.name != "nt":
        assert os.access(impa.impa_executable, os.X_OK)
        
    assert resolved == impa.impa_executable

@mock.patch("subprocess.Popen")
def test_build_invokes_command_with_correct_args(mock_popen, temp_impa):
    mock_process = mock.MagicMock()
    mock_process.returncode = 0
    mock_process.stdout = []
    mock_process.stderr = []
    mock_popen.return_value = mock_process
    
    success = temp_impa.build(
        include=["component1", "component2"],
        components_dir="custom_components"
    )
    
    assert success is True
    mock_popen.assert_called_once()
    
    args, kwargs = mock_popen.call_args
    cmd = args[0]
    
    assert temp_impa.impa_executable in cmd
    assert "build" in cmd
    assert "--root-dir" in cmd
    assert temp_impa.root_dir in cmd
    assert "--manifest-filename" in cmd
    assert temp_impa.manifest_filename in cmd
    assert "--include" in cmd
    assert "component1,component2" in cmd
    assert "--components-dir" in cmd
    assert "custom_components" in cmd

@mock.patch("subprocess.Popen")
def test_run_passes_config_via_stdin_and_collects_stdout(mock_popen, temp_impa):
    mock_process = mock.MagicMock()
    mock_process.returncode = 0
    
    stdout_lines = [
        '{"task_index":0,"executor":"test","metric":12.3}\n',
        '{"task_index":1,"executor":"test","metric":45.6}\n'
    ]
    mock_process.stdout = iter(stdout_lines)
    mock_process.stderr = iter([])
    mock_process.stdin = mock.MagicMock()
    mock_popen.return_value = mock_process
    
    config_dict = {
        "generator": {"name": "test-gen"},
        "tasks": [{"executor": "test"}]
    }
    
    results = temp_impa.run(**config_dict)
    
    assert len(results) == 2
    assert results[0]["executor"] == "test"
    assert results[0]["metric"] == 12.3
    assert results[1]["metric"] == 45.6
    
    mock_popen.assert_called_once()
    args, kwargs = mock_popen.call_args
    cmd = args[0]
    
    assert "run" in cmd
    assert "--config" in cmd
    assert "-" in cmd
    
    mock_process.stdin.write.assert_called_once()
    written_data = mock_process.stdin.write.call_args[0][0]
    parsed_written_data = json.loads(written_data)
    assert parsed_written_data["generator"]["name"] == "test-gen"

def test_lab_parsing():
    lab_jsonl = Lab(MOCK_RESULTS_JSONL)
    assert len(lab_jsonl.results) == 3
    assert lab_jsonl.results[0]["executor"] == "zig-executors"
    assert lab_jsonl.results[1]["metric"] == 200
    
    lab_list = Lab(MOCK_RESULTS_LIST)
    assert len(lab_list.results) == 3
    assert lab_list.results[2]["metric"] == 460
    
    lab_array = Lab(json.dumps(MOCK_RESULTS_LIST))
    assert len(lab_array.results) == 3
    
    with pytest.raises(TypeError):
        Lab(123)

def test_lab_dataframe_conversion():
    lab = Lab(MOCK_RESULTS_LIST)
    
    try:
        import pandas as pd
        has_pandas = True
    except ImportError:
        has_pandas = False
        
    if has_pandas:
        df = lab.to_dataframe()
        assert isinstance(df, pd.DataFrame)
        assert len(df) == 3
        assert df["task_label"].iloc[0] == "zig-executors linear_search"
        assert df["task_label"].iloc[1] == "zig-executors binary_search"
        assert df["attr.cpu"].iloc[0] == "x86_64"
        assert df["gen.size"].iloc[0] == 100
        assert df["exec.iters"].iloc[0] == 10
        assert pd.isna(df["exec.iters"].iloc[1]) # binary_search doesn't have exec_meta
    else:
        with pytest.raises(ImportError):
            lab.to_dataframe()

def test_lab_summary_and_best():
    lab = Lab(MOCK_RESULTS_LIST)
    
    summary = lab.summary()
    best = lab.best()
    
    if isinstance(summary, list):
        assert len(summary) == 2
        # sorted by mean: binary_search (mean 200) then linear_search (mean 455)
        assert summary[0]["task_label"] == "zig-executors binary_search"
        assert summary[0]["mean"] == 200.0
        assert summary[1]["task_label"] == "zig-executors linear_search"
        assert summary[1]["mean"] == 455.0 # (450 + 460) / 2
        assert summary[1]["count"] == 2
        
        assert best["task_label"] == "zig-executors binary_search"
    else:
        assert len(summary) == 2
        assert summary["task_label"].iloc[0] == "zig-executors binary_search"
        assert summary["mean"].iloc[0] == 200.0
        assert summary["task_label"].iloc[1] == "zig-executors linear_search"
        assert summary["mean"].iloc[1] == 455.0
        assert summary["count"].iloc[1] == 2
        
        assert best["task_label"] == "zig-executors binary_search"
        assert best["mean"] == 200.0

def test_lab_summary_no_pandas_fallback():
    # Force summary to use manual fallback by hiding pandas
    lab = Lab(MOCK_RESULTS_LIST)
    
    with mock.patch.dict("sys.modules", {"pandas": None}):
        summary = lab.summary()
        best = lab.best()
        
        assert isinstance(summary, list)
        assert len(summary) == 2
        assert summary[0]["task_label"] == "zig-executors binary_search"
        assert summary[0]["mean"] == 200.0
        assert summary[1]["task_label"] == "zig-executors linear_search"
        assert summary[1]["mean"] == 455.0
        assert summary[1]["count"] == 2
        
        assert best["task_label"] == "zig-executors binary_search"

@mock.patch("subprocess.Popen")
@mock.patch("tqdm.tqdm")
def test_run_tqdm_progress_terminal(mock_tqdm_cls, mock_popen, temp_impa):
    mock_process = mock.MagicMock()
    mock_process.returncode = 0
    mock_process.stdout = iter([
        '{"task_index":0,"executor":"test","metric":12.3}\n',
        '{"task_index":1,"executor":"test","metric":45.6}\n'
    ])
    mock_process.stderr = iter([])
    mock_process.stdin = mock.MagicMock()
    mock_popen.return_value = mock_process

    mock_pbar = mock.MagicMock()
    mock_tqdm_cls.return_value = mock_pbar

    config_dict = {
        "reps": 2,
        "tasks": [
            {"executor": "test", "reps": 3},
            {"executor": "test2"} # falls back to global reps = 2
        ]
    }
    # Expected total runs = 3 + 2 = 5
    results = temp_impa.run(pbar_total=5, **config_dict)

    mock_tqdm_cls.assert_called_once_with(total=5, desc="Running benchmarks")
    assert mock_pbar.update.call_count == 2
    mock_pbar.update.assert_has_calls([mock.call(1), mock.call(1)])
    mock_pbar.close.assert_called_once()

@mock.patch("subprocess.Popen")
@mock.patch("tqdm.notebook.tqdm")
def test_run_tqdm_progress_jupyter(mock_tqdm_notebook_cls, mock_popen, temp_impa):
    # Simulate being in Jupyter environment
    import sys
    mock_ipython = mock.MagicMock()
    mock_ip = mock.MagicMock()
    mock_ip.__class__.__name__ = 'ZMQInteractiveShell'
    mock_ipython.get_ipython.return_value = mock_ip

    mock_process = mock.MagicMock()
    mock_process.returncode = 0
    mock_process.stdout = iter([
        '{"task_index":0,"executor":"test","metric":12.3}\n'
    ])
    mock_process.stderr = iter([])
    mock_process.stdin = mock.MagicMock()
    mock_popen.return_value = mock_process

    mock_pbar = mock.MagicMock()
    mock_tqdm_notebook_cls.return_value = mock_pbar

    config_dict = {
        "tasks": [{"executor": "test"}] # reps default to 1
    }
    with mock.patch.dict("sys.modules", {"IPython": mock_ipython}):
        results = temp_impa.run(pbar_total=1, **config_dict)

    mock_tqdm_notebook_cls.assert_called_once_with(total=1, desc="Running benchmarks")
    mock_pbar.update.assert_called_once_with(1)
    mock_pbar.close.assert_called_once()

@mock.patch("subprocess.Popen")
def test_run_tqdm_graceful_fallback(mock_popen, temp_impa):
    mock_process = mock.MagicMock()
    mock_process.returncode = 0
    mock_process.stdout = iter([
        '{"task_index":0,"executor":"test","metric":12.3}\n'
    ])
    mock_process.stderr = iter([])
    mock_process.stdin = mock.MagicMock()
    mock_popen.return_value = mock_process

    with mock.patch.dict("sys.modules", {"tqdm": None}):
        config_dict = {
            "tasks": [{"executor": "test"}]
        }
        # This should execute successfully without raising ImportError
        results = temp_impa.run(**config_dict)
        assert len(results) == 1

