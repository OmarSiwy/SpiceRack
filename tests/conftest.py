import sys
from pathlib import Path


PYTHON_SRC = Path(__file__).resolve().parents[1] / "python"
if str(PYTHON_SRC) not in sys.path:
    sys.path.insert(0, str(PYTHON_SRC))
