import numpy as np
from math import sqrt
from pathlib import Path as P


def clean_alias_and_scope(data):
    root = P(".")
    vector = np.array([1, 2, 3])
    first, second = (1, 2)

    with open(root / "sample.txt", "w") as handle:
        handle.write(str(sqrt(16)))

    total = first + second
    for key, value in data.items():
        print(key, value)

    try:
        int("42")
    except ValueError as exc:
        print(exc)

    return vector.tolist(), total
