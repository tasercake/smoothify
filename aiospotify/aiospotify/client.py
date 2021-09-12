from __future__ import annotations
from typing import Optional, Dict, List, TypeVar, Generic
from pydantic import BaseModel, Extra
from pydantic.generics import GenericModel

import json
from datetime import datetime

import requests
import spotipy

T = TypeVar("T")


class SpotifyObject(BaseModel):
    class Config:
        extra = Extra.allow
    client: Spotify


class TrackObject(SpotifyObject):
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


class SavedTrackObject(SpotifyObject):
    added_at: datetime
    track: TrackObject


class SpotifyPagingObject(SpotifyObject, GenericModel, Generic[T]):
    href: str
    items: List[T]
    limit: int
    next: Optional[str]
    offset: int
    previous: Optional[str]
    total: int

    async def __anext__():
        pass

    async def __aiter__():
        pass


class Spotify:
    def __init__(self, *, auth: str):
        # TODO: Port to httpx
        self.client = spotipy.Spotify(auth=auth)
        # ===== WIP: Porting to httpx =====
        self.prefix = "https://api.spotify.com/v1/"
        self._auth = auth

    # region Spotify API methods
    # TODO: Port to use async HTTP methods
    async def current_user_saved_tracks(self, limit: int = 50, offset: int = 0, market: str = None) -> SpotifyPagingObject[SavedTrackObject]:
        result = self.client.current_user_saved_tracks(limit=limit, offset=offset, market=market)
        return SpotifyPagingObject(**result)

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

    # region HTTP Methods
    async def _get(self, url, args=None, payload=None, **kwargs):
        if args:
            kwargs.update(args)

        return await self._internal_call("GET", url, payload, kwargs)

    async def _post(self, url, args=None, payload=None, **kwargs):
        if args:
            kwargs.update(args)
        return await self._internal_call("POST", url, payload, kwargs)

    async def _delete(self, url, args=None, payload=None, **kwargs):
        if args:
            kwargs.update(args)
        return await self._internal_call("DELETE", url, payload, kwargs)

    async def _put(self, url, args=None, payload=None, **kwargs):
        if args:
            kwargs.update(args)
        return await self._internal_call("PUT", url, payload, kwargs)

    async def _internal_call(self, method, url, payload, params):
        args = dict(params=params)
        if not url.startswith("http"):
            url = self.prefix + url
        headers = self._auth_headers()

        if "content_type" in args["params"]:
            headers["Content-Type"] = args["params"]["content_type"]
            del args["params"]["content_type"]
            if payload:
                args["data"] = payload
        else:
            headers["Content-Type"] = "application/json"
            if payload:
                args["data"] = json.dumps(payload)

        if self.language is not None:
            headers["Accept-Language"] = self.language

        logger.debug('Sending %s to %s with Params: %s Headers: %s and Body: %r ',
                     method, url, args.get("params"), headers, args.get('data'))

        try:
            response = self._session.request(
                method, url, headers=headers, proxies=self.proxies,
                timeout=self.requests_timeout, **args
            )

            response.raise_for_status()
            results = response.json()
        except requests.exceptions.HTTPError as http_error:
            response = http_error.response
            try:
                json_response = response.json()
                error = json_response.get("error", {})
                msg = error.get("message")
                reason = error.get("reason")
            except ValueError:
                # if the response cannnot be decoded into JSON (which raises a ValueError),
                # then try to decode it into text

                # if we receive an empty string (which is falsy), then replace it with `None`
                msg = response.text or None
                reason = None

            logger.error(
                'HTTP Error for %s to %s with Params: %s returned %s due to %s',
                method, url, args.get("params"), response.status_code, msg
            )

            raise SpotifyException(
                response.status_code,
                -1,
                "%s:\n %s" % (response.url, msg),
                reason=reason,
                headers=response.headers,
            )
        except requests.exceptions.RetryError as retry_error:
            request = retry_error.request
            logger.error('Max Retries reached')
            try:
                reason = retry_error.args[0].reason
            except (IndexError, AttributeError):
                reason = None
            raise SpotifyException(
                429,
                -1,
                "%s:\n %s" % (request.path_url, "Max Retries"),
                reason=reason
            )
        except ValueError:
            results = None

        logger.debug('RESULTS: %s', results)
        return results

    @property
    def _auth_headers(self):
        return {"Authorization": "Bearer {0}".format(self._auth)}
    # endregion
