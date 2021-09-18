from datetime import datetime

from aiospotify.resources.spotify_object import SpotifyObject
from aiospotify.resources.spotify_track import SpotifyTrack


class SpotifySavedTrack(SpotifyObject):
    added_at: datetime
    track: SpotifyTrack

