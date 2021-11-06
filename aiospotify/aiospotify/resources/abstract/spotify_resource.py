from __future__ import annotations

from typing import TYPE_CHECKING, Generic, Optional, Type, TypeVar

from aiospotify.models.abstract.spotify_object import SpotifyObject

if TYPE_CHECKING:
    from aiospotify.async_client import AsyncSpotify


T = TypeVar("T", bound=SpotifyObject)


class SpotifyResource(Generic[T]):
    path: str
    response_type: Type[T]
    max_limit: int

    def __init__(self, *, client: AsyncSpotify):
        self.client = client

    async def get(self, *, offset: int = 0, limit: Optional[int] = None) -> T:
        params = dict(offset=offset, limit=limit or self.max_limit)
        data = await self.client.request("GET", self.path, params=params)
        return self.response_type(**data)
