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
  ],
  callbacks: {
    jwt: async function jwt(token, user) {
      if (user) {
        token = { accessToken: user.accessToken }
      }
      return token
    },
    session: async function session(session, token) {
      session.accessToken = token.accessToken
      return session
    }
  },
  secret: 'H0DUIJz74dZ2Mo2scXfTs146slYpLV1jcscUVo0NcohbFubKpdSBn0tHn46GLzDi'
})

export default nextAuthHandler
