from aiospotify.models.spotify_user import SpotifyUser
from aiospotify.resources.abstract import GettableResource


class CurrentUser(GettableResource[SpotifyUser]):
    path = "/me"
    response_type = SpotifyUser
