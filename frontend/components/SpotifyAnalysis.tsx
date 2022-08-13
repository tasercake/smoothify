import { useState } from 'react'
import useSWR from 'swr'
import spotifyClient from '../lib/spotifyClient'

interface SpotifySearchParams {
  query?: string
  type: 'artist' | 'track'
}

interface TrackSearchItem {
  album: {}
  artists: {}[]
  available_markets: string[]
  disc_number: number
  duration_ms: number
  explicit: boolean
  href: string
  id: string
  name: string
  popularity: number
  preview_url: string
  track_number: number
  type: string
  uri: string
}

interface TrackSearchResponse {
  tracks: {
    href: string
    items: TrackSearchItem[]
    limit: number
    next: string | null
    offset: number
    previous: string | null
    total: number
  }
}

const spotifySearchFetcher = async (
  params: SpotifySearchParams,
  accessToken: string
): Promise<TrackSearchResponse | undefined> => {
  const { query } = params
  if (!query) return
  const resp = await spotifyClient.get<TrackSearchResponse>('/v1/search', {
    params,
    headers: { Authorization: `Bearer: ${accessToken}` }
  })
  return resp.data
}

const SpotifyAnalysis = () => {
  const [searchQuery, setSearchQuery] = useState<string>()
  // TODO: get token
  const token = ''
  const { data } = useSWR(searchQuery ?? null, (query?: string) =>
    token ? spotifySearchFetcher({ query, type: 'track' }, token) : undefined
  )

  const searchItems = data?.tracks.items

  return (
    <>
      <section>
        <input
          type="text"
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
        />
      </section>
      {searchItems && (
        <section>
          {searchItems.map((trackInfo) => (
            <a key={trackInfo.id} href={trackInfo.href}>
              {trackInfo.name}
            </a>
          ))}
        </section>
      )}
    </>
  )
}

export default SpotifyAnalysis
