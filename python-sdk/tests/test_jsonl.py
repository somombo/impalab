import pytest
import sys
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
