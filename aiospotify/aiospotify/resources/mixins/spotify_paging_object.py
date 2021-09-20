from __future__ import annotations
from typing import (
    Awaitable,
    Optional,
    List,
    Callable,
    TypeVar,
    Generic,
    runtime_checkable,
)
from pydantic.generics import GenericModel

from aiospotify.resources.spotify_object import SpotifyObject

T = TypeVar("T")


class SpotifyPagingObject(SpotifyObject, GenericModel, Generic[T]):
    href: str
    items: List[T]
    limit: int
    next: Optional[str]
    offset: int
    previous: Optional[str]
    total: int
    get_next: Callable[[], Awaitable[SpotifyPagingObject[T]]]


SpotifyPagingObject.update_forward_refs()
