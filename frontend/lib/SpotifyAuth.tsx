import React, { createContext, useContext } from 'react'

export interface SpotifyAuthProps {}

export const SpotifyAuthContext = createContext<SpotifyAuthProps | undefined>(
  undefined
)

export const SpotifyAuthProvider: React.FC = ({ children }) => {
  return (
    <SpotifyAuthContext.Provider value={{}}>
      {children}
    </SpotifyAuthContext.Provider>
  )
}

export const useSpotifyAuth = () => {
  const context = useContext(SpotifyAuthContext)
  if (context === undefined) {
    throw new Error('useSpotifyAuth must be used within a SpotifyAuthProvider')
  }
  return context
}
