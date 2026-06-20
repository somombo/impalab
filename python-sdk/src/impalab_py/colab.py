import os
import sys
import subprocess

def is_colab_env() -> bool:
  """
  check if we are in Google Colab Hosted Runtime Environment
  """
  try:
    import google.colab
    return ("COLAB_RELEASE_TAG" in os.environ) or ("COLAB_GPU" in os.environ)
  except ImportError:
    return False


def get_git_root():
  try:
    return subprocess.check_output(['git', 'rev-parse', '--show-toplevel'], text=True ).strip()
  except subprocess.CalledProcessError:
    return None


def colab_repo_sync(repo_url = None, repo_subdir = "."):
  repo_path = get_git_root()
  if repo_path:
    os.chdir(repo_path)
  else:
    if is_colab_env() and repo_url is not None:
      repo_name = repo_url.rstrip('/').split('/')[-1]
      if repo_name.endswith('.git'):
        repo_name = repo_name[:-4]
      repo_path = repo_name or "cloned_repo"

      if not os.path.exists(repo_path):
        subprocess.run(['git', 'clone', repo_url, repo_path], check=True)
        print("Cloning complete.")

      os.chdir(repo_path)

  os.chdir(repo_subdir)
  
  current_dir = os.getcwd()
  if current_dir not in sys.path:
    sys.path.insert(0, current_dir)

if is_colab_env(): # check if we are in Google Colab Hosted Runtime Environment
    from google.colab import output
    output.enable_custom_widget_manager()
    print("Custom widget manager has been enabled. It can be disabled with `google.colab.output.disable_custom_widget_manager()`")

