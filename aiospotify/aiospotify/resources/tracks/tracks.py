from aiospotify.models.spotify_track import SpotifyTrack
from aiospotify.resources.abstract import GettableResource


class Tracks(GettableResource[SpotifyTrack]):
    path = "/tracks"
    response_type = SpotifyTrack
