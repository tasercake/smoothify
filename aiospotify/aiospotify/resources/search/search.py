from aiospotify.models.spotify_search_result import SpotifySearchResult
from aiospotify.resources.abstract import GettableResource


class Search(GettableResource[SpotifySearchResult]):
    path = "/search"
    response_type = SpotifySearchResult
