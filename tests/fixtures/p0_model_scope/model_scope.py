from __future__ import annotations

from typing import Optional


class BaseModel:
    pass


class ItemResponse(BaseModel):
    id: str
    name: str
    parent: Optional[ParentResponse] = None


class ParentResponse(BaseModel):
    id: str
    model_config = {"from_attributes": True}


class Repository:
    async def fetch_item(self, connection: object, **payload: object) -> object:
        row = await connection.fetchrow("SELECT 1", payload["id"])
        return row
