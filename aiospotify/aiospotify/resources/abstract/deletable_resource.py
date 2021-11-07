from __future__ import annotations

from typing import Generic, Type, TypeVar

from aiospotify.models.abstract.spotify_object import SpotifyObject
from aiospotify.resources.abstract.resource import Resource

T = TypeVar("T", bound=SpotifyObject)


class DeletableResource(Resource, Generic[T]):
    # TODO: Define generic response type correctly
    response_type: Type[T]

    # TODO: Implement resource deletion
    async def delete(self, *args, **kwargs) -> T:
        raise NotImplementedError()
