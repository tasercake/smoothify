from typing import Optional, List

from aiospotify.models.abstract.spotify_object import SpotifyObject


class Track(SpotifyObject):
    available_markets: List[str]
    disc_number: int
    duration_ms: int
    explicit: bool
    href: str
    id: str
    is_local: bool
    is_playable: Optional[bool]
    name: str
    popularity: int
    preview_url: Optional[str]
    track_number: int
    type: str
    uri: str
