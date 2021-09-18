from typing import List

from aiospotify.resources.spotify_object import SpotifyObject


class SpotifyTrack(SpotifyObject):
    available_markets: List[str]
    disc_number: int
    duration_ms: int
    explicit: bool
    href: str
    id: str
    is_local: bool
    is_playable: bool
    name: str
    popularity: int
    preview_url: str
    track_number: int
    type: str
    uri: str
