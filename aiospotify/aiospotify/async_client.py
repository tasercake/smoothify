from __future__ import annotations

from typing import Any, Dict, Optional

import aiohttp

from aiospotify.auth import SpotifyAuth
from aiospotify.resources.library.current_user_saved_tracks import (
    CurrentUserSavedTracks,
)

from aiospotify.resources.tracks.audio_analysis import AudioAnalysis
from aiospotify.resources.tracks.audio_features import AudioFeatures
from aiospotify.resources.tracks.tracks import Tracks
from aiospotify.resources.users.current_user import CurrentUser
from aiospotify.resources.users.user import User


class AsyncSpotify:
    base_url: str = "https://api.spotify.com/v1"

    def __init__(self, *, auth: SpotifyAuth, session: aiohttp.ClientSession) -> None:
        self.auth = auth
        self.session = session

    # region Resources
    @property
    def current_user_saved_tracks(self) -> CurrentUserSavedTracks:
        return CurrentUserSavedTracks(client=self)

    @property
    def audio_features(self) -> AudioFeatures:
        return AudioFeatures(client=self)

    @property
    def audio_analysis(self) -> AudioAnalysis:
        return AudioAnalysis(client=self)

    @property
    def tracks(self) -> Tracks:
        return Tracks(client=self)

    @property
    def current_user(self) -> CurrentUser:
        return CurrentUser(client=self)

    @property
    def user(self) -> User:
        return User(client=self)

    # endregion

    # region HTTP Methods
    async def request(
        self, method: str, url: str, *, suffix: Optional[str] = None, **kwargs
    ) -> Dict[str, Any]:
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
        if suffix:
            url = f"{url}/{suffix}"

        # Inject auth headers
        auth_headers = await self.auth.get_auth_headers()
        kwargs.setdefault("headers", {}).update(auth_headers)

        # Send request
        async with self.session.request(method, url, **kwargs) as response:
            # Parse response body as JSON and return it
            return await response.json()

    # endregion
