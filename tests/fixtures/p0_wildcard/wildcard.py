from pathlib import *


def uses_wildcard():
    path = Path(".")
    return path.exists()
