import NextAuth from 'next-auth'
import Providers from 'next-auth/providers'
import {
  SPOTIFY_AUTH_SCOPES,
  SPOTIFY_CLIENT_ID,
  SPOTIFY_CLIENT_SECRET
} from '../../../lib/constants'

const nextAuthHandler = NextAuth({
  debug: process.env.NODE_ENV !== 'production',
  // Configure one or more authentication providers
  providers: [
    Providers.Spotify({
      clientId: SPOTIFY_CLIENT_ID,
      clientSecret: SPOTIFY_CLIENT_SECRET,
      scope: SPOTIFY_AUTH_SCOPES,
      authorizationUrl:
        'https://accounts.spotify.com/authorize?response_type=code&show_dialog=true',
      profile(profile, tokens) {
        console.log('profile')
        console.log(profile)
        console.log(tokens)
        return {
          id: profile.id as string,
          name: profile.display_name as string,
          email: profile.email,
          accessToken: tokens.accessToken,
          refreshToken: tokens.refreshToken
        }
      }
    })
  ],
  session: {
    jwt: true
  },
  callbacks: {
    jwt: async (token, user, account, profile) => {
      console.log('jwt')
      console.log(profile)
      token = { accessToken: profile?.accessToken }
      return token
    },
    session: async (session, token) => {
      console.log('session')
      console.log(session)
      console.log(token)
      session.accessToken = token.accessToken
      return session
    },
    signIn(user, account, profile) {
      return true
    }
  },
  secret: 'H0DUIJz74dZ2Mo2scXfTs146slYpLV1jcscUVo0NcohbFubKpdSBn0tHn46GLzDi'
})

export default nextAuthHandler
