import React from 'react'
import { Navigate } from 'react-router-dom'

export const getHash = () => {
  return window
    ? window.location.hash
        .substring(1)
        .split('&')
        .reduce((initial, item) => {
          if (item) {
            const parts = item.split('=')
            // @ts-ignore
            initial[parts[0]] = decodeURIComponent(parts[1])
          }
          return initial
        }, {})
    : ''
}

const SpotifyImplicitGrantCallbackPage: React.FC = () => {
  const token = (getHash() as any).access_token
  localStorage.setItem('spotifyAuthToken', token)
  if (!token) return <h1>No token</h1>
  return <Navigate to="/" />
}
export default SpotifyImplicitGrantCallbackPage
