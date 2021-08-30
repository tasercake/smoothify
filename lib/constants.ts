// Client
export const BASE_URL = process.env.BASE_URL

// Spotify API
export const SPOTIFY_API_BASE_URL = 'https://api.spotify.com'
export const SPOTIFY_CLIENT_ID = process.env.SPOTIFY_CLIENT_ID
export const SPOTIFY_CLIENT_SECRET = process.env.SPOTIFY_CLIENT_SECRET
export const SPOTIFY_AUTH_SCOPES = [
  'user-read-email',
  'user-library-read',
  'user-read-recently-played',
  'user-top-read',
  'playlist-modify-public',
  'playlist-read-private',
  'playlist-modify-private',
  'playlist-read-collaborative'
]
