from aiospotify.resources.abstract import ListableResource
from aiospotify.resources.abstract import CreatableResource
from aiospotify.resources.abstract import DeletableResource
from aiospotify.models.abstract.spotify_paging_object import SpotifyPagingObject
from aiospotify.models.spotify_saved_track import SpotifySavedTrack


class CurrentUserSavedTracks(
    ListableResource[SpotifySavedTrack],
    CreatableResource[SpotifySavedTrack],
    DeletableResource[SpotifySavedTrack],
):
    path = "/me/tracks"
    response_type = SpotifyPagingObject[SpotifySavedTrack]
    max_limit = 50

    # TODO: Implement creation and deletion
