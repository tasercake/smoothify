from datetime import datetime

from aiospotify.resources.abstract.spotify_object import SpotifyObject
from aiospotify.resources.spotify_track import Track


class SavedTrack(SpotifyObject):
    added_at: datetime
    track: Track
