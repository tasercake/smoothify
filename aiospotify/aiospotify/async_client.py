from __future__ import annotations

from typing import Any, Dict, Type

import aiohttp

from aiospotify.auth import SpotifyAuth
from aiospotify.models.abstract.spotify_object import SpotifyObject
from aiospotify.resources.abstract.spotify_resource import SpotifyResource
from aiospotify.resources.audio_features import AudioFeatures
from aiospotify.resources.current_user_saved_tracks import CurrentUserSavedTracks


class AsyncSpotify:
    base_url: str = "https://api.spotify.com/v1"

    # TODO: Provide this info during type-checking
    resource_map: Dict[str, Type[SpotifyResource]] = {
        "audio_features": AudioFeatures,
        "current_user_saved_tracks": CurrentUserSavedTracks,
    }

    def __getattr__(self, key):
        if key in self.resource_map:
            resource = self.get_resource(key)
            return resource
        else:
            return super().__getattr__(key)

    @classmethod
    def get_resource_cls(cls, resource_key: str) -> Type[SpotifyResource]:
        resource_cls = cls.resource_map[resource_key]
        return resource_cls

    def get_resource(self, resource_key: str) -> SpotifyResource:
        resource_cls = self.get_resource_cls(resource_key)
        resource = resource_cls(client=self)
        return resource

    def __init__(self, *, auth: SpotifyAuth, session: aiohttp.ClientSession) -> None:
        self.auth = auth
        self.session = session

    @property
    def current_user_saved_tracks(self) -> CurrentUserSavedTracks:
        return CurrentUserSavedTracks(client=self)

    @property
    def audio_features(self) -> AudioFeatures:
        return AudioFeatures(client=self)

    # region HTTP Methods
    async def request(self, method: str, url: str, **kwargs) -> Dict[str, Any]:
        """Send a HTTP request to the Spotify API."""
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

    # region Context Manager API
    async def __aenter__(self) -> AsyncSpotify:
        return self

    async def __aexit__(self) -> None:
        await self.close()

    async def close(self) -> None:
        await self.session.close()

    # endregion
