from typing import List

from fastapi import status, Query
from fastapi_utils.inferring_router import InferringRouter
from smoothify.core import SmoothifyManager
from smoothify.optim import KDTreeBottleneckTSP

# TODO: Move Spotify dependency into `smoothify` package
from tekore import Spotify

router = InferringRouter()


@router.post("/library", status_code=status.HTTP_200_OK)
async def smoothify_library(spotify_token: str = Query(...)) -> str:
    """
    Smoothify user library
    """
    spotify = Spotify(
        spotify_token, asynchronous=True, max_limits_on=True, chunked_on=True
    )
    manager = SmoothifyManager(spotify=spotify, smoothifier=KDTreeBottleneckTSP())
    return await manager.smoothify_user_library()


@router.post("/playlist/{playlist_id}", status_code=status.HTTP_200_OK)
async def smoothify_playlist(playlist_id: str, spotify_token: str = Query(...)) -> str:
    """
    Smoothify a playlist
    """
    spotify = Spotify(
        spotify_token, asynchronous=True, max_limits_on=True, chunked_on=True
    )
    manager = SmoothifyManager(spotify=spotify, smoothifier=KDTreeBottleneckTSP())
    return await manager.smoothify_playlist(playlist_id)


@router.get("/scopes", status_code=status.HTTP_200_OK)
async def required_scopes() -> List[str]:
    """
    Get the list of Spotify scopes required for the app
    """
    return [
        # Read
        "user-library-read",
        "playlist-read-private",
        # Write
        "playlist-modify-private",
        "playlist-modify-public",
    ]
