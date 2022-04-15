from typing import Optional, List

import numpy as np
import structlog
from tekore import Spotify
from tekore.model import FullPlaylist, PrivateUser, AudioFeatures

from smoothify.features import SpotifyFeatureConstructor
from smoothify.optim import Smoothifier

logger = structlog.get_logger()


class SmoothifyManager:
    def __init__(self, *, spotify: Spotify, smoothifier: Smoothifier):
        self.spotify = spotify
        self.smoothifier = smoothifier
        # Cache
        self._current_user: Optional[PrivateUser] = None

    async def smoothify_user_library(self) -> None:
        """
        Smoothify a user's library of favorite tracks.
        """
        # Get current user
        current_user = await self._get_current_user()
        logger.info(f"Will smoothify {current_user.display_name}'s library")
        # Get user's tracks
        logger.info("Getting user's tracks")
        saved_tracks_page = await self.spotify.saved_tracks()
        track_list = [
            track.track async for track in self.spotify.all_items(saved_tracks_page)
        ]
        logger.info(f"Got {len(track_list)} tracks")

        # Get track audio features
        track_id_list = [track.id for track in track_list]
        logger.info("Fetching track audio features")
        audio_features_list = await self.spotify.tracks_audio_features(track_id_list)

        # Construct points from features
        points = self._construct_points_from_features(audio_features_list)

        # Find optimal path
        logger.info("Finding optimal path through points")
        results = self.smoothifier.get_best_path(points=points)
        best_path = results.best_path

        # Create smoothified playlist
        smoothified_track_uris = [track_list[node_idx].uri for node_idx in best_path]
        await self.create_playlist_with_tracks(
            user_id=current_user.id,
            playlist_name=f"{current_user.display_name}'s Library (but smoother)",
            track_uris=smoothified_track_uris,
            public=False,
            collaborative=False,
        )

    async def smoothify_playlist(self, playlist_id: str) -> None:
        current_user = await self._get_current_user()

        # Get playlist tracks
        logger.info(f"Fetching playlist {playlist_id}")
        playlist: FullPlaylist = await self.spotify.playlist(playlist_id)
        logger.info(f"Will smoothify {playlist.name}")
        track_list = [
            track.track async for track in self.spotify.all_items(playlist.tracks)
        ]
        logger.info(f"Got {len(track_list)} tracks")

        # Get track audio features
        track_id_list = [track.id for track in track_list]
        logger.info("Fetching track audio features")
        audio_features_list = await self.spotify.tracks_audio_features(track_id_list)

        # Construct points from features
        points = self._construct_points_from_features(audio_features_list)

        # Find optimal path
        logger.info("Finding optimal path through points")
        results = self.smoothifier.get_best_path(points=points)
        best_path = results.best_path

        # Create smoothified playlist
        smoothified_track_uris = [track_list[node_idx].uri for node_idx in best_path]
        await self.create_playlist_with_tracks(
            user_id=current_user.id,
            playlist_name=f"{playlist.name} (but smoother)",
            track_uris=smoothified_track_uris,
            public=False,
            collaborative=False,
        )

    def _construct_points_from_features(
        self, audio_features_list: List[AudioFeatures]
    ) -> np.ndarray:
        logger.info("Constructing points array from features")
        feature_constructor = SpotifyFeatureConstructor(audio_features_list)
        features_df = feature_constructor.construct_features()
        points = np.array(features_df)
        return points

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
        new_playlist = await self.spotify.playlist_create(
            user_id, playlist_name, public=public
        )
        await self.spotify.playlist_change_details(
            new_playlist.id, collaborative=collaborative
        )

        # Add tracks to new playlist
        await self.spotify.playlist_add(new_playlist.id, track_uris)

    async def _get_current_user(self) -> PrivateUser:
        if not self._current_user:
            logger.info("Fetching Spotify user")
            self._current_user = await self.spotify.current_user()
        return self._current_user
