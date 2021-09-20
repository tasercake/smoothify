from abc import ABC, abstractmethod


class SpotifyAuth(ABC):
    @abstractmethod
    async def get_access_token(self):
        """Return the current access token if it's available and valid.
        If a valid access token isn't available (or is expired), request a new one from Spotify.

        Returns:
            str: An access token for the Spotify API
        """

    async def get_auth_headers(self):
        return {"Authorization": f"Bearer {await self.get_access_token()}"}


class SpotifyPKCE(SpotifyAuth):
    def __init__(self, *, token: str):
        self.token = token

    async def get_access_token(self):
        return self.token
