from __future__ import annotations
from typing import Optional, List, TypeVar, Generic
from pydantic.generics import GenericModel

from aiospotify.models.abstract.spotify_object import SpotifyObject

T = TypeVar("T")


class SpotifyPagingObject(SpotifyObject, GenericModel, Generic[T]):
    href: str
    items: List[T]
    limit: int
    next: Optional[str]
    offset: int
    previous: Optional[str]
    total: int


SpotifyPagingObject.update_forward_refs()
