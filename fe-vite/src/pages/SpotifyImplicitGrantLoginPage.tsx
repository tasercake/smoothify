import React from 'react'
import {
  SPOTIFY_ACCOUNTS_BASE_URL,
  SPOTIFY_AUTH_SCOPES,
  SPOTIFY_CLIENT_ID,
  SPOTIFY_LOGIN_CALLBACK_PATH
} from '../lib/constants'

/**
 * Compute SHA256 hash of a string.
 * To be used for PKCE code verifier (currently not used for Implicit Grant flow).
 */
const sha256 = async (message: string) => {
  // encode as UTF-8
  const msgBuffer = new TextEncoder().encode(message)

  // hash the message
  const hashBuffer = await crypto.subtle.digest('SHA-256', msgBuffer)

  // convert ArrayBuffer to Array
  const hashArray = Array.from(new Uint8Array(hashBuffer))

  // convert bytes to hex string
  return hashArray.map((b) => b.toString(16).padStart(2, '0')).join('')
}

/**
 * Implicit grant flow login for Spotify.
 * This is a simplified login flow that doesn't support refresh tokens.
 * TODO: Port to Authorization flow + PKCE for better security & refresh token support.
 */
const login = async () => {
  // TODO: !!! Persist state (in localStorage?) & compare against state in redirect URL to protect against CSRF
  const state = crypto.randomUUID() + crypto.randomUUID()
  const params = new URLSearchParams({
    response_type: 'token', // TODO: Change to 'code' for Authorization flow + PKCE
    client_id: SPOTIFY_CLIENT_ID,
    redirect_uri: `${window.location.origin}/${SPOTIFY_LOGIN_CALLBACK_PATH}`,
    scope: SPOTIFY_AUTH_SCOPES,
    state: state
  })
  const destination = new URL(
    `/authorize?${params.toString()}`,
    SPOTIFY_ACCOUNTS_BASE_URL
  )
  window.location.href = destination.toString()
}

const SpotifyImplicitGrantLoginPage: React.FC = () => {
  return <button onClick={login}>Login</button>
}
export default SpotifyImplicitGrantLoginPage
