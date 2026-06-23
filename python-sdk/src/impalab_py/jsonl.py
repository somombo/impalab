import json
import sys
from typing import List, Dict, Any

def loads(s: str) -> List[Dict[str, Any]]:
    if not isinstance(s, str):
        raise TypeError("results must be a JSONL string")
    
    parsed = []
    for line in s.splitlines():
        line_stripped = line.strip()
        if line_stripped:
            try:
                parsed.append(json.loads(line_stripped))
            except json.JSONDecodeError:
                print(f"Warning: could not parse stdout line as JSON: {line_stripped}", file=sys.stderr)
                continue
    return parsed

def dumps(obj: List[Dict[str, Any]]) -> str:
    if not isinstance(obj, list):
        raise TypeError("obj must be a list of dictionaries")
    if not obj:
        return ""
    return join([json.dumps(item) for item in obj])

def join(lines: List[str]) -> str:
    if not lines:
        return ""
    
    stripped_lines = []
    for line in lines:
        stripped = line.strip()
        if stripped:
            stripped_lines.append(stripped)
            
    if not stripped_lines:
        return ""
        
    return "\n".join(stripped_lines) + "\n"
