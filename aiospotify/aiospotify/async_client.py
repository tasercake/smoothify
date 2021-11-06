from __future__ import annotations

import asyncio
from math import ceil
from typing import AsyncGenerator, Dict, Generic, List, Optional, Type, TypeVar

import aiohttp

from aiospotify.auth import SpotifyAuth
from aiospotify.resources.abstract.spotify_object import SpotifyObject
from aiospotify.resources.abstract.spotify_paging_object import SpotifyPagingObject
from aiospotify.resources.spotify_saved_track import SavedTrack

T = TypeVar("T", bound=SpotifyObject)


class SpotifyResource(Generic[T]):
    path: str
    response_type: Type[T]
    max_limit: int

    def __init__(self, *, client: AsyncSpotify):
        self.client = client

    async def get(self, *, offset: int = 0, limit: int = None) -> T:
        params = dict(offset=offset, limit=limit or self.max_limit)
        data = await self.client.request("GET", self.path, params=params)
        return self.response_type(**data)


class ListableResource(SpotifyResource, Generic[T]):
    response_type: Type[SpotifyPagingObject[T]]

    async def next(
        self, *, prev: SpotifyPagingObject[T]
    ) -> Optional[SpotifyPagingObject[T]]:
        next_href = prev.next
        if not next_href:
            return None
        data = await self.client.request("GET", next_href)
        return self.response_type(**data)

    async def iterate(self, *, offset: int = 0) -> AsyncGenerator[T, None]:
        has_next = True
        while has_next:
            page = await self.get(offset=offset)
            items = self._extract_items(page)
            has_next = bool(page.next)
            for item in items:
                yield item
            offset += self.max_limit

    async def get_all(self, *, offset: int = 0) -> List[T]:
        data_list: List[T] = []
        # Fetch first page and get pagination info
        response = await self.get(offset=offset)
        total_items = response.total
        data_list.extend(response.items)

        # If there are no remaining items, no further requests are made
        remaining_items = total_items - len(data_list)
        num_requests = ceil(remaining_items / self.max_limit)
        offsets = [
            offset + len(data_list) + (i * self.max_limit) for i in range(num_requests)
        ]
        # Fetch all remaining pages concurrently
        pages: List[SpotifyPagingObject[T]] = await asyncio.gather(
            *[self.get(offset=o) for o in offsets]
        )
        for page in pages:
            data_list.extend(page.items)

        return data_list

    @classmethod
    def _extract_items(cls, page: SpotifyPagingObject):
        return page.items


class CurrentUserSavedTracks(ListableResource[SavedTrack]):
    path = "/me/tracks"
    response_type = SpotifyPagingObject[SavedTrack]
    max_limit = 50


class AudioFeatures(SpotifyResource):
    pass


class AsyncSpotify:
    base_url: str = "https://api.spotify.com/v1"

    # TODO: Provide this info during type-checking
    resource_map: Dict[str, Type[SpotifyResource]] = {
        "current_user_saved_tracks": CurrentUserSavedTracks,
        "audio_features": AudioFeatures,
    }

    def __getattr__(self, key):
        if key in self.resource_map:
            resource = self.get_resource(key)
            return resource
        else:
            return super().__getattr__(key)

    @classmethod
    def get_resource_cls(cls, resource_key: str):
        resource_cls = cls.resource_map[resource_key]
        return resource_cls

    def get_resource(self, resource_key: str):
        resource_cls = self.get_resource_cls(resource_key)
        resource = resource_cls(client=self)
        return resource

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
