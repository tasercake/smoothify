from __future__ import annotations
from typing import Optional, List, Type, TypeVar, Generic
from abc import ABC, abstractmethod
from math import ceil
from aiospotify.resources.abstract.spotify_object import SpotifyObject

import aiohttp

from aiospotify.auth import SpotifyAuth
from aiospotify.resources.abstract.spotify_paging_object import SpotifyPagingObject
from aiospotify.resources.spotify_saved_track import SavedTrack

T = TypeVar("T")

# TODO: Rename to something better - this name isn't right but it shouldn't conflict with the SpotifyObject naming scheme
class SpotifyResource(ABC, Generic[T]):
    ResponseType = SpotifyObject

    def __init__(self, *, client: AsyncSpotify):
        self.client = client

    @abstractmethod
    @property
    def response_type(self) -> Type:
        return int

    @abstractmethod
    @property
    def path(self) -> str:
        ...

    @abstractmethod
    @property
    def max_limit(self) -> int:
        ...

    async def get(self, *, offset: int = 0, limit: int = None) -> T:
        params = dict(offset=offset, limit=limit or self.max_limit)
        data = await self.client.request("GET", self.path, params=params)
        return self.response_type(**data)


class ListableResource(SpotifyResource):
    # response_type = SpotifyPagingObject[SavedTrack]

    path = "/me/tracks"
    max_limit = 50

    async def get(self, *, offset: int = 0, limit: int = None) -> response_type:
        params = dict(offset=offset, limit=limit or self.max_limit)
        data = await self.client.request("GET", self.path, params=params)
        return self.response_type(**data)


# TODO: CurrentUserSavedTracks should subclass ListableResource
class CurrentUserSavedTracks(SpotifyResource):
    ResponseType = SpotifyPagingObject[SavedTrack]

    path = "/me/tracks"
    max_limit = 50

    async def get(self, *, offset: int = 0, limit: int = None) -> ResponseType:
        params = dict(offset=offset, limit=limit or self.max_limit)
        data = await self.client.request("GET", self.path, params=params)
        return self.ResponseType(**data)

    async def next(self, *, prev: ResponseType) -> Optional[ResponseType]:
        next_href = prev.next
        if not next_href:
            return None
        data = await self.client.request("GET", next_href)
        return self.ResponseType(**data)

    async def get_all_items(self, *, offset: int = 0) -> List[SavedTrack]:
        data_list: List[SavedTrack] = []
        first = await self.get(offset=offset)
        items = first.items
        data_list.extend(items)

        num_remaining_items = first.total - len(items)
        if num_remaining_items > 0:
            num_requests = ceil(num_remaining_items / self.max_limit)
            offsets = [offset + (i * self.max_limit) for i in range(num_requests)]

        return data_list


class AudioFeatures(SpotifyResource):
    pass


class AsyncSpotify:
    base_url: str = "https://api.spotify.com/v1"

    def __init__(
        self,
        *,
        auth: SpotifyAuth,
        session: aiohttp.ClientSession,
    ) -> None:
        self.auth = auth
        self.session = session

    @property
    def current_user_saved_tracks(self):
        return CurrentUserSavedTracks(client=self)

    @property
    def audio_features(self):
        return AudioFeatures(client=self)

    # region HTTP Methods
    async def request(self, method: str, url: str, **kwargs):
        """Send a HTTP request to the Spotify API.

        Args:
            method (str): The HTTP verb to use
            path (str):
        """
        # If we get a full URL, check that it's for the Spotify API
        is_spotify_api_url = url.startswith(self.base_url)
        if url.startswith("https://"):
            if not is_spotify_api_url:
                raise ValueError(f"Expected a Spotify API URL, but got {url}")
        else:
            # Remove leading slash if present (we add it back later)
            url = url.lstrip("/")
            # Prepend Spotify API base URL
            url = f"{self.base_url}/{url}"

        # Inject auth headers
        auth_headers = await self.auth.get_auth_headers()
        kwargs.setdefault("headers", {}).update(auth_headers)

        # Send request
        async with self.session.request(method, url, **kwargs) as response:
            # Parse response body as JSON and return it
            return await response.json()

    # endregion

    # region HTTP helpers

    # endregion

    # region Context Manager API
    async def __aenter__(self):
        return self

    async def __aexit__(self):
        await self.close()

    async def close(self):
        await self.session.close()

    # endregion
