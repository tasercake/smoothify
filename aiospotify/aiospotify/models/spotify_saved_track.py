from datetime import datetime

from aiospotify.models.abstract.spotify_object import SpotifyObject
from aiospotify.models.spotify_track import Track


class SavedTrack(SpotifyObject):
    added_at: datetime
    track: Track
