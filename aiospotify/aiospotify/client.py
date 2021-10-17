from typing import Optional, List, Callable, TypeVar

from functools import partial

import spotipy

from aiospotify.resources.spotify_saved_track import SavedTrack
from aiospotify.resources.abstract.spotify_paging_object import SpotifyPagingObject

T = TypeVar("T")


class Spotify:
    def __init__(self, *, auth: str):
        self.client = spotipy.Spotify(auth=auth)

    # region Spotify API methods
    # TODO: Port to use async HTTP methods
    async def current_user_saved_tracks(
        self, limit: int = 50, offset: int = 0, market: str = None
    ) -> SpotifyPagingObject[SavedTrack]:
        result = self.client.current_user_saved_tracks(
            limit=limit, offset=offset, market=market
        )
        get_next = partial(
            self.current_user_saved_tracks,
            limit=limit,
            offset=offset + len(result["items"]),
            market=market,
        )
        return SpotifyPagingObject(**result, get_next=get_next)

    async def audio_features(self, tracks: List[str]):
        max_tracks = 100
        return self.client.audio_features(tracks)

    async def audio_analysis(self, track_id: str):
        return self.client.audio_analysis(track_id)

    # endregion

    # region Helper methods
    async def next(self, result: SpotifyPagingObject[T]) -> SpotifyPagingObject[T]:
        # TODO: Refactor to allow API like `result.next()`
        return SpotifyPagingObject[T](**self.client.next(result.dict()))

    async def get_all(self, result: SpotifyPagingObject[T]) -> List[T]:
        # TODO: Refactor to allow API like `result.all()`
        all_items = result.items
        while result.next:
            new_result = await self.next(result)
            all_items += new_result.items
            result = new_result
        return all_items

    # endregion
