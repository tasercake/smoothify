from typing import Optional

import aiohttp

from aiospotify.auth import SpotifyAuth


class AsyncSpotify:
    prefix: str = "https://api.spotify.com/v1"

    def __init__(
        self,
        *,
        auth: SpotifyAuth,
        max_limits: bool = True,
        connector: aiohttp.BaseConnector = None,
        connector_owner: bool = True,
    ) -> None:
        self.auth = auth
        self.max_limits = max_limits
        self.session = None
        self.session = aiohttp.ClientSession(
            raise_for_status=True, connector=connector, connector_owner=connector_owner
        )

    # TODO: Figure out how to max out limits
    def current_user_saved_tracks(self, *, offset):
        pass

    # region HTTP Methods
    async def request(self, method: str, path: str, **kwargs):
        path = path.lstrip("/")

        # Inject auth headers
        auth_headers = await self.auth.get_auth_headers()
        kwargs.setdefault("headers", {}).update(auth_headers)

        # Construct full URL and send request
        url = f"{self.prefix}/{path}"
        async with self.session.request(method, url, **kwargs) as response:
            # Parse response body as JSON and return it
            return await response.json()

    # endregion

    # region Context Manager API
    async def __aenter__(self):
        return self

    async def __aexit__(self):
        await self.close()

    async def close(self):
        await self.session.close()

    # endregion
