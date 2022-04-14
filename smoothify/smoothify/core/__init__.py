from typing import Type, Optional, List

import numpy as np
import tekore as tk

from smoothify.features import SpotifyFeatureConstructor
from smoothify.optim import Smoothifier


class SmoothifyManager:
    def __init__(
        self,
        *,
        smoothifier_cls: Type[Smoothifier],
        access_token: Optional[str] = None,
    ):
        self._access_token = access_token
        self.smoothifier_cls = smoothifier_cls

        # Cache
        self._spotify: Optional[tk.Spotify] = None
        self._current_user: Optional[tk.model.PrivateUser] = None

    @property
    def spotify(self):
        if not self._spotify:
            self._spotify = tk.Spotify(await self.get_access_token(), asynchronous=True, max_limits_on=True, chunked_on=True)
        return self._spotify

    async def get_access_token(self):
        pass

    async def get_current_user(self) -> tk.model.PrivateUser:
        if not self._current_user:
            self._current_user = await self.spotify.current_user()
        return self._current_user

    async def smoothify_user_library(self) -> None:
        # Get user's tracks
        current_user = await self.get_current_user()
        saved_tracks_page = await self.spotify.saved_tracks()
        track_list = [track.track async for track in self.spotify.all_items(saved_tracks_page)]

        # Get track audio features
        track_id_list = [track.id for track in track_list]
        audio_features_list = await self.spotify.tracks_audio_features(track_id_list)

        # Construct features
        feature_constructor = SpotifyFeatureConstructor(audio_features_list=audio_features_list)
        features_df = feature_constructor.construct_features()
        points = np.array(features_df)

        # Find optimal path
        optimizer = self.smoothifier_cls(points=points)
        results = optimizer.get_best_path()
        best_path = results.best_path

        # Create smoothified playlist
        new_playlist_name = f"{current_user.display_name}'s Library (but smoother)"
        smoothified_track_uris = [track_list[node_idx].uri for node_idx in best_path]
        await self.create_playlist_with_tracks(
            user_id=current_user.id,
            playlist_name=new_playlist_name,
            track_uris=smoothified_track_uris,
            public=False,
            collaborative=False,
        )

    def smoothify_playlist(self, playlist_id: str) -> None:
        current_user = await self.get_current_user()

        # Get playlist tracks
        playlist: tk.model.FullPlaylist = await self.spotify.playlist(playlist_id)
        track_list = [track.track async for track in self.spotify.all_items(playlist.tracks)]

        # Get track audio features
        track_id_list = [track.id for track in track_list]
        audio_features_list = await self.spotify.tracks_audio_features(track_id_list)

        # Construct features
        feature_constructor = SpotifyFeatureConstructor(audio_features_list=audio_features_list)
        features_df = feature_constructor.construct_features()
        points = np.array(features_df)

        # Find optimal path
        optimizer = self.smoothifier_cls(points=points)
        results = optimizer.get_best_path()
        best_path = results.best_path

        # Create smoothified playlist
        new_playlist_name = f"{playlist.name} (but smoother)"
        smoothified_track_uris = [track_list[node_idx].uri for node_idx in best_path]
        await self.create_playlist_with_tracks(
            user_id=current_user.id,
            playlist_name=new_playlist_name,
            track_uris=smoothified_track_uris,
            public=False,
            collaborative=False,
        )

    async def create_playlist_with_tracks(
        self,
        *,
        user_id: str,
        playlist_name: str,
        track_uris: List[str],
        public: bool = False,
        collaborative: bool = False,
    ) -> None:
        # Create new playlist
        new_playlist = await self.spotify.playlist_create(user_id, playlist_name, public=public)
        await self.spotify.playlist_change_details(new_playlist.id, collaborative=collaborative)

        # Add tracks to new playlist
        await self.spotify.playlist_add(new_playlist.id, track_uris)
