from aiospotify.models.spotify_playlist import SpotifyPlaylist
from aiospotify.resources.abstract import ListableResource


class CurrentUserPlaylists(ListableResource[SpotifyPlaylist]):
    path = "/me/playlists"
    response_type = SpotifyPlaylist
