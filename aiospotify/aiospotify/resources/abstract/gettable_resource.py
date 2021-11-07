from __future__ import annotations

from typing import Optional, Generic, TypeVar

from aiospotify.models.abstract.spotify_object import SpotifyObject
from aiospotify.resources.abstract.resource import Resource

T = TypeVar("T", bound=SpotifyObject)


class GettableResource(Resource, Generic[T]):
    async def get(self, *, offset: int = 0, limit: Optional[int] = None, **kwargs) -> T:
        params = dict(offset=offset, limit=limit or self.max_limit)
        data = await self.client.request("GET", self.path, params=params, **kwargs)
        return self.response_type(**data)
