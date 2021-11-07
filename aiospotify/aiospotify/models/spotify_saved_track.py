from datetime import datetime

from aiospotify.models.abstract.spotify_object import SpotifyObject
from aiospotify.models.spotify_track import SpotifyTrack


class SpotifySavedTrack(SpotifyObject):
    added_at: datetime
    track: SpotifyTrack
