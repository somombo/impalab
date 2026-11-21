"""Read and write the JSONL text that ``impa run`` prints, one JSON object per line.

Nothing here needs pandas, so results can be saved and loaded without the
``data`` extra.
"""

import gzip
import json
import os
import sys
import urllib.request
import uuid
import warnings
from typing import List, Dict, Any

GZIP_MAGIC = b"\x1f\x8b"

def loads(s: str) -> List[Dict[str, Any]]:
    """Parse JSONL text into a list of objects, skipping blank lines and warning about malformed ones."""
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
    """Serialize a list of objects as JSONL text, one object per line."""
    if not isinstance(obj, list):
        raise TypeError("obj must be a list of dictionaries")
    if not obj:
        return ""
    return join([json.dumps(item) for item in obj])

def join(lines: List[str]) -> str:
    """Join JSONL lines, or whole JSONL texts, into one JSONL text without blank lines between them."""
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

def write(path: str | os.PathLike, text: str) -> None:
    """Save JSONL text, such as the string ``Impa.run`` returns, to ``path``.

    A path ending in ``.gz`` is compressed with gzip. Missing parent
    directories are created and an existing file is replaced. The data is on
    disk before it replaces the previous file, so if saving fails the previous
    file is left as it was. Syncing the directory afterwards puts the rename
    on disk too; if only that step fails, this warns instead of raising,
    because the new file is already saved.
    """
    if not isinstance(text, str):
        raise TypeError("text must be a JSONL string")
    path = os.fspath(path)
    parent = os.path.dirname(path)
    if parent:
        os.makedirs(parent, exist_ok=True)

    data = text.encode("utf-8")
    if path.endswith(".gz"):
        data = gzip.compress(data)

    # Write beside the destination and rename over it, so a failed save keeps the
    # previous file. The leading dot keeps the temporary file out of glob matches.
    tmp = os.path.join(parent, f".{os.path.basename(path)}.{uuid.uuid4().hex}.tmp")
    try:
        with open(tmp, "xb") as f:
            f.write(data)
            f.flush()
            # The OS writes file data back lazily, up to about 30 s later. Sync now so
            # that disk activity cannot overlap the next benchmark run.
            os.fsync(f.fileno())
        os.replace(tmp, path)
    except BaseException:
        if os.path.exists(tmp):
            os.remove(tmp)
        raise
    try:
        _sync_directory(parent or os.curdir)
    except OSError as e:
        warnings.warn(f"saved {path}, but could not sync its directory: {e}", RuntimeWarning, stacklevel=2)

def _sync_directory(path: str) -> None:
    # The rename is a change to the directory, so sync it too. Windows can't open a directory to sync it.
    if os.name == "nt":
        return
    fd = os.open(path, os.O_RDONLY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)

def read(path: str | os.PathLike) -> str:
    """Load JSONL text from a file path or an ``http(s)://`` URL.

    Gzip-compressed data is decompressed whatever its name, so files saved
    with a ``.gz`` path and downloads with query strings both work.
    """
    path = os.fspath(path)
    if path.startswith(("http://", "https://")):
        with urllib.request.urlopen(path) as response:
            data = response.read()
    else:
        with open(path, "rb") as f:
            data = f.read()
    if data.startswith(GZIP_MAGIC):
        data = gzip.decompress(data)
    return data.decode("utf-8")
