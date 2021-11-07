from aiospotify.models.abstract.spotify_object import SpotifyObject
from aiospotify.models.abstract.spotify_paging_object import SpotifyPagingObject
from aiospotify.resources.abstract import ListableResource

SpotifyTopItems = SpotifyObject


class CurrentUserTopItems(ListableResource[SpotifyTopItems]):
    path = "/me/top"
    response_type = SpotifyPagingObject[SpotifyTopItems]
    # TODO: Define parameters for endpoint
