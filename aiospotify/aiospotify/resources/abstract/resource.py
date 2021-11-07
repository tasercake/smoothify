from __future__ import annotations

from typing import TYPE_CHECKING, Dict, Generic, List, Optional, Type, TypeVar, Union

from aiospotify.models.abstract.spotify_object import SpotifyObject

if TYPE_CHECKING:
    from aiospotify.async_client import AsyncSpotify


T = TypeVar("T", bound=SpotifyObject)
Primitive = Union[str, int, float, bool]


class Resource(Generic[T]):
    path: str
    response_type: Type[T]
    max_limit: int

    # Slots for parameters to send to the resource endpoint
    path_param_slots: Optional[List[Primitive]] = None
    query_param_slots: Optional[Dict[str, Union[Primitive, List[Primitive]]]] = None

    def __init__(self, *, client: AsyncSpotify):
        self.client = client
