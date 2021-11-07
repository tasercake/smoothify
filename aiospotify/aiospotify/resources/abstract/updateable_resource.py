from __future__ import annotations

from typing import Generic, Type, TypeVar

from aiospotify.models.abstract.spotify_object import SpotifyObject
from aiospotify.resources.abstract.resource import Resource

T = TypeVar("T", bound=SpotifyObject)


class UpdatableResource(Resource, Generic[T]):
    # TODO: Define generic response type correctly
    response_type: Type[T]

    # TODO: Implement resource updating
    async def update(self, *args, **kwargs) -> T:
        raise NotImplementedError()
