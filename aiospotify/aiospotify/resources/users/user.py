from aiospotify.models.spotify_user import SpotifyUser
from aiospotify.resources.abstract import GettableResource


class User(GettableResource[SpotifyUser]):
    path = "/users"
    response_type = SpotifyUser
    # TODO: Define parameters for endpoint
