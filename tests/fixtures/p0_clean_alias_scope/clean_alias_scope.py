import numpy as np
from math import sqrt
from pathlib import Path as P
from urllib.request import urlopen

API_VAULT_SECRET_REF_AUTH_SHARED_SECRET = "vault:runtime/api_auth_shared_secret"
SUPPRESSED_EXAMPLE_KEY = "sk-proj-suppressed-placeholder"  # nosec


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


def valid_local_attribute_access(dashboard, path, scopes, operations):
    with urlopen("http://localhost:8000/health", timeout=5) as response:
        status = response.status
        body = response.read().decode("utf-8", errors="replace")

    titles = {panel.get("title") for panel in dashboard.get("panels", [])}
    segments = [
        segment
        for segment in path.strip("/").split("/")
        if segment and not segment.startswith("{") and not segment.endswith("}")
    ]
    scope_keys = [
        f"{scope.resource_type}:{scope.resource_id}:{scope.tenant_id or '*'}"
        for scope in scopes
    ]
    budgets = [entry.latency_budget_ms for entry in operations if entry.latency_budget_ms]
    latest = sorted(titles, key=lambda item: item.lower())

    return (
        status,
        body,
        segments,
        scope_keys,
        budgets,
        latest,
        API_VAULT_SECRET_REF_AUTH_SHARED_SECRET,
    )


async def valid_async_local_attribute_access(pool, client, settings):
    async with pool.acquire() as connection:
        row = await connection.fetchrow("SELECT 1 AS ok")

    async with client.stream("GET", settings.health_url) as response:
        payload = await response.aread()

    return row.get("ok"), payload.decode("utf-8")
