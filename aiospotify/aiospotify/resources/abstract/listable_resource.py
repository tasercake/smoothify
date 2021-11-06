from __future__ import annotations

import asyncio
from math import ceil
from typing import TYPE_CHECKING, AsyncGenerator, Generic, List, Optional, Type, TypeVar

from aiospotify.models.abstract.spotify_object import SpotifyObject
from aiospotify.models.abstract.spotify_paging_object import SpotifyPagingObject
from aiospotify.resources.abstract.spotify_resource import SpotifyResource

T = TypeVar("T", bound=SpotifyObject)


class ListableResource(SpotifyResource, Generic[T]):
    response_type: Type[SpotifyPagingObject[T]]

    async def next(
        self, *, prev: SpotifyPagingObject[T]
    ) -> Optional[SpotifyPagingObject[T]]:
        next_href = prev.next
        if not next_href:
            return None
        data = await self.client.request("GET", next_href)
        return self.response_type(**data)

    async def iterate(self, *, offset: int = 0) -> AsyncGenerator[T, None]:
        has_next = True
        while has_next:
            page = await self.get(offset=offset)
            items = self._extract_items(page)
            has_next = bool(page.next)
            for item in items:
                yield item
            offset += self.max_limit

    async def get_all(self, *, offset: int = 0) -> List[T]:
        data_list: List[T] = []
        # Fetch first page and get pagination info
        response = await self.get(offset=offset)
        total_items = response.total
        data_list.extend(response.items)

        # If there are no remaining items, no further requests are made
        remaining_items = total_items - len(data_list)
        num_requests = ceil(remaining_items / self.max_limit)
        offsets = [
            offset + len(data_list) + (i * self.max_limit) for i in range(num_requests)
        ]
        # Fetch all remaining pages concurrently
        pages: List[SpotifyPagingObject[T]] = await asyncio.gather(
            *[self.get(offset=o) for o in offsets]
        )
        for page in pages:
            data_list.extend(page.items)

        return data_list

    @classmethod
    def _extract_items(cls, page: SpotifyPagingObject) -> List[T]:
        return page.items
