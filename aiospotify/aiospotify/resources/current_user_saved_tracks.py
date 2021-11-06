from aiospotify.resources.abstract.listable_resource import ListableResource
from aiospotify.models.abstract.spotify_paging_object import SpotifyPagingObject
from aiospotify.models.spotify_saved_track import SavedTrack


class CurrentUserSavedTracks(ListableResource[SavedTrack]):
    path = "/me/tracks"
    response_type = SpotifyPagingObject[SavedTrack]
    max_limit = 50
