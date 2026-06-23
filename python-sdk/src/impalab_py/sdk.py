import os
import sys
import json
import subprocess
import urllib.request
import shutil
import stat
from typing import List, Dict, Any, Optional

from . import jsonl

class Impa:
    def __init__(
        self,
        root_dir: str = ".",
        manifest_filename: str = "impa_manifest.json",
        bin_dir: Optional[str] = None,
    ):
        self.root_dir = os.path.abspath(root_dir)
        self.manifest_filename = manifest_filename
        self.impa_executable = self._resolve_impa_executable(bin_dir)
        print(f"Proposed path to `impa` executable: '{self.impa_executable}'", file=sys.stderr)


    def _resolve_impa_executable(self, bin_dir: Optional[str]) -> str:
        exe_name = "impa.exe" if os.name == "nt" else "impa"

        if bin_dir:
            abs_path = os.path.abspath(bin_dir.strip())
            candidate = os.path.join(abs_path, exe_name)
            if not os.path.isfile(candidate):
                raise RuntimeError(f"bin_dir is set, but no {exe_name} executable was found at '{candidate}'")
            return candidate

        local_candidate = os.path.join(self.root_dir, ".bin", exe_name)
        if os.path.isfile(local_candidate):
            return local_candidate

        in_path = shutil.which("impa")
        if in_path:
            return in_path

        env_val = os.environ.get("IMPALAB_BIN_DIR")
        if env_val is not None:
            abs_path = os.path.abspath(env_val.strip())
            candidate = os.path.join(abs_path, exe_name)
            if not os.path.isfile(candidate):
                raise RuntimeError(f"IMPALAB_BIN_DIR environment variable is set, but no {exe_name} executable was found at '{candidate}'")
            return candidate

        return os.path.join(self.root_dir, ".bin", exe_name)

    def _ensure_executable(self) -> str:
        if os.path.isfile(self.impa_executable):
            if os.name != "nt":
                st = os.stat(self.impa_executable)
                os.chmod(self.impa_executable, st.st_mode | stat.S_IEXEC)
            return self.impa_executable

        raise FileNotFoundError(f"'impa' executable not found at '{self.impa_executable}'. Please call `download_executable()` to install it.")

    def download_executable(self, version_tag: str = "v0.5.1", target: Optional[str] = None) -> str:
        print(f"Downloading 'impa' version '{version_tag}' from GitHub...", file=sys.stderr)
        
        exe_name = "impa.exe" if os.name == "nt" else "impa"
        download_path = os.path.join(self.root_dir, ".bin", exe_name)

        if target:
            url = f"https://github.com/somombo/impalab/releases/download/{version_tag}/{target}"
        else:
            url = f"https://github.com/somombo/impalab/releases/download/{version_tag}/impa-linux-amd64"
            if os.name == "nt":
                url = f"https://github.com/somombo/impalab/releases/download/{version_tag}/impa-windows-amd64.exe"
            
        parent_dir = os.path.dirname(download_path)
        if parent_dir:
            os.makedirs(parent_dir, exist_ok=True)
            
        try:
            # Add User-Agent header to avoid potential rate limit blocking by GitHub API/CDN
            req = urllib.request.Request(
                url, 
                headers={'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36'}
            )
            with urllib.request.urlopen(req) as response, open(download_path, "wb") as out_file:
                shutil.copyfileobj(response, out_file)
            
            if os.name != "nt":
                st = os.stat(download_path)
                os.chmod(download_path, st.st_mode | stat.S_IEXEC)
            
            self.impa_executable = download_path
            print(f"Downloaded 'impa' successfully to {self.impa_executable}", file=sys.stderr)
        except Exception as e:
            raise RuntimeError(f"Failed to download 'impa' from {url}: {e}") from e

        return self.impa_executable

    def build(
        self,
        include: Optional[List[str]] = None,
        exclude: Optional[List[str]] = None,
        components_dir: Optional[str] = None,
    ) -> bool:
        exe = self._ensure_executable()
        
        cmd = [exe, "build", "--root-dir", self.root_dir]
        if self.manifest_filename:
            cmd.extend(["--manifest-filename", self.manifest_filename])
            
        if components_dir:
            cmd.extend(["--components-dir", components_dir])
            
        if include:
            cmd.extend(["--include", ",".join(include)])
        elif exclude:
            cmd.extend(["--exclude", ",".join(exclude)])
            
        try:
            process = subprocess.Popen(
                cmd,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                bufsize=1,
                cwd=self.root_dir
            )
            
            import threading
            
            def stream_pipe(pipe, out_stream):
                for line in pipe:
                    out_stream.write(line)
                    out_stream.flush()
                    
            stderr_thread = threading.Thread(target=stream_pipe, args=(process.stderr, sys.stderr))
            stdout_thread = threading.Thread(target=stream_pipe, args=(process.stdout, sys.stdout))
            
            stderr_thread.start()
            stdout_thread.start()
            
            process.wait()
            stderr_thread.join()
            stdout_thread.join()
            
            return process.returncode == 0
        except Exception as e:
            print(f"Error running build command: {e}", file=sys.stderr)
            return False

    def run(self, pbar_total = 0, **config) -> str:
        exe = self._ensure_executable()
        
        cmd = [exe, "run", "--root-dir", self.root_dir, "--config", "-"]
        if self.manifest_filename:
            cmd.extend(["--manifest-filename", self.manifest_filename])
            
        config_json = json.dumps(config)
        try:
            process = subprocess.Popen(
                cmd,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                bufsize=1,
                cwd=self.root_dir
            )
            
            import threading
            def stream_stderr(pipe):
                for line in pipe:
                    sys.stderr.write(line)
                    sys.stderr.flush()
                    
            stderr_thread = threading.Thread(target=stream_stderr, args=(process.stderr,))
            stderr_thread.start()
            
            process.stdin.write(config_json)
            process.stdin.close()
            
            results = []

            pbar = None
            try:
                is_jupyter = False
                try:
                    from IPython import get_ipython
                    ip = get_ipython()
                    if ip is not None and ip.__class__.__name__ == 'ZMQInteractiveShell':
                        is_jupyter = True
                except (ImportError, NameError):
                    pass

                if is_jupyter:
                    from tqdm.notebook import tqdm
                else:
                    from tqdm import tqdm

                pbar = tqdm(total=pbar_total, desc="Running benchmarks")
            except Exception:
                pass

            try:
                for line in process.stdout:
                    line_stripped = line.strip()
                    if line_stripped:
                        results.append(line_stripped)
                        if pbar is not None:
                            pbar.update(1)
                
                process.wait()
                stderr_thread.join()
                
                if process.returncode != 0:
                    raise RuntimeError(f"impa run failed with exit code {process.returncode}")
                    
                return jsonl.join(results)
            finally:
                if pbar is not None:
                    pbar.close()
        except Exception as e:
            if not isinstance(e, RuntimeError):
                raise RuntimeError(f"Error running impa: {e}") from e
            raise
