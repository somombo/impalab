import gzip
import os
import pytest
import sys
import threading
from unittest import mock
from impalab_py import jsonl

def test_loads_valid_jsonl():
    jsonl_str = '{"a": 1}\n{"b": 2}\n'
    result = jsonl.loads(jsonl_str)
    assert len(result) == 2
    assert result[0]["a"] == 1
    assert result[1]["b"] == 2

def test_loads_ignores_empty_lines():
    jsonl_str = '\n  \n{"a": 1}\n\n'
    result = jsonl.loads(jsonl_str)
    assert len(result) == 1
    assert result[0]["a"] == 1

def test_loads_type_error():
    with pytest.raises(TypeError, match="results must be a JSONL string"):
        jsonl.loads([{"a": 1}])

def test_loads_malformed_json_warning():
    jsonl_str = '{"a": 1}\nmalformed_json\n{"b": 2}'
    
    with mock.patch("sys.stderr.write") as mock_stderr_write:
        result = jsonl.loads(jsonl_str)
        assert len(result) == 2
        assert result[0]["a"] == 1
        assert result[1]["b"] == 2
        
        # Verify that a warning was written to stderr
        assert mock_stderr_write.call_count > 0
        written_text = "".join(call.args[0] for call in mock_stderr_write.call_args_list)
        assert "Warning: could not parse stdout line as JSON: malformed_json" in written_text

def test_join_valid_lines():
    lines = ['{"a": 1}', '  {"b": 2}  ', '{"c": 3}\n']
    result = jsonl.join(lines)
    assert result == '{"a": 1}\n{"b": 2}\n{"c": 3}\n'

def test_join_ignores_empty_lines():
    lines = ['{"a": 1}', '', '  \n ', '{"b": 2}']
    result = jsonl.join(lines)
    assert result == '{"a": 1}\n{"b": 2}\n'

def test_join_empty_list():
    assert jsonl.join([]) == ""

def test_dumps_valid_list():
    obj = [{"a": 1}, {"b": 2}]
    result = jsonl.dumps(obj)
    assert result == '{"a": 1}\n{"b": 2}\n'

def test_dumps_empty_list():
    assert jsonl.dumps([]) == ""

def test_dumps_type_error():
    with pytest.raises(TypeError, match="obj must be a list of dictionaries"):
        jsonl.dumps('{"a": 1}')

@pytest.mark.parametrize("name", ["results.jsonl", "results.jsonl.gz"])
def test_write_then_read_round_trips_the_text(tmp_path, name):
    text = '{"a": 1}\n{"b": 2}\n'
    path = tmp_path / name

    jsonl.write(path, text)

    assert path.read_bytes().startswith(jsonl.GZIP_MAGIC) == name.endswith(".gz")
    assert jsonl.read(path) == text

def test_write_creates_parent_directories_and_replaces_existing_files(tmp_path):
    path = tmp_path / "data" / "study" / "results.jsonl"

    jsonl.write(path, '{"old": 1}\n')
    jsonl.write(path, '{"new": 2}\n')

    assert path.read_text() == '{"new": 2}\n'
    assert list(path.parent.iterdir()) == [path]

@pytest.mark.skipif(os.name == "nt", reason="Windows can't sync a directory")
def test_write_syncs_the_file_before_moving_it_into_place_then_syncs_the_directory(tmp_path):
    path = tmp_path / "results.jsonl"
    at_sync = []

    def record(fd):
        at_sync.append((path.exists(), [p.read_text() for p in tmp_path.iterdir()]))

    with mock.patch("os.fsync", side_effect=record):
        jsonl.write(path, '{"a": 1}\n')

    assert at_sync == [(False, ['{"a": 1}\n']), (True, ['{"a": 1}\n'])]

def test_write_keeps_the_previous_file_when_saving_fails(tmp_path):
    path = tmp_path / "results.jsonl"
    jsonl.write(path, '{"previous": 1}\n')

    with mock.patch("os.fsync", side_effect=OSError("disk full")):
        with pytest.raises(OSError, match="disk full"):
            jsonl.write(path, '{"next": 2}\n')

    assert list(tmp_path.iterdir()) == [path]
    assert path.read_text() == '{"previous": 1}\n'

def test_concurrent_writes_to_one_path_do_not_interfere(tmp_path):
    path = tmp_path / "results.jsonl"
    texts = ['{"a": 1}\n', '{"b": 2}\n']
    both_written = threading.Barrier(len(texts), timeout=10)
    synced_once = set()
    real_fsync = os.fsync
    errors = []

    def hold_first_sync(fd):
        # Each writer waits at its first sync until both have written, so the saves overlap.
        if threading.get_ident() not in synced_once:
            synced_once.add(threading.get_ident())
            both_written.wait()
        real_fsync(fd)

    def save(text):
        try:
            jsonl.write(path, text)
        except Exception as e:
            errors.append(e)

    with mock.patch("os.fsync", side_effect=hold_first_sync):
        threads = [threading.Thread(target=save, args=(text,)) for text in texts]
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join()

    assert errors == []
    assert path.read_text() in texts
    assert list(tmp_path.iterdir()) == [path]

def test_write_rejects_anything_but_text(tmp_path):
    with pytest.raises(TypeError, match="JSONL string"):
        jsonl.write(tmp_path / "results.jsonl", [{"a": 1}])

def test_read_fetches_urls_and_decompresses_gzip_whatever_the_name():
    url = "https://example.com/download?file=results"
    response = mock.MagicMock()
    response.__enter__.return_value.read.return_value = gzip.compress(b'{"a": 1}\n')

    with mock.patch("urllib.request.urlopen", return_value=response) as urlopen:
        text = jsonl.read(url)

    urlopen.assert_called_once_with(url)
    assert text == '{"a": 1}\n'

@pytest.mark.skipif(os.name == "nt", reason="Windows can't sync a directory")
@pytest.mark.parametrize("failing", ["open", "sync"])
def test_write_warns_but_keeps_the_new_file_when_only_the_directory_sync_fails(tmp_path, failing):
    path = tmp_path / "results.jsonl"
    jsonl.write(path, '{"previous": 1}\n')
    real_fsync = os.fsync
    synced = []

    def fail_directory_sync(fd):
        synced.append(fd)
        if len(synced) == 2:
            raise OSError("cannot sync")
        real_fsync(fd)

    patch = (
        mock.patch("os.open", side_effect=PermissionError("cannot read directory"))
        if failing == "open"
        else mock.patch("os.fsync", side_effect=fail_directory_sync)
    )
    with patch, pytest.warns(RuntimeWarning, match="could not sync its directory"):
        jsonl.write(path, '{"next": 2}\n')

    assert path.read_text() == '{"next": 2}\n'
    assert list(tmp_path.iterdir()) == [path]
