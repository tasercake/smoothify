import NextAuth from 'next-auth'
import Providers from 'next-auth/providers'
import {
  SPOTIFY_AUTH_SCOPES,
  SPOTIFY_CLIENT_ID,
  SPOTIFY_CLIENT_SECRET
} from '../../../lib/constants'

const nextAuthHandler = NextAuth({
  // Configure one or more authentication providers
  providers: [
    Providers.Spotify({
      clientId: SPOTIFY_CLIENT_ID,
      clientSecret: SPOTIFY_CLIENT_SECRET,
      scope: SPOTIFY_AUTH_SCOPES
    })
  ]
})

export default nextAuthHandler
