import '../styles/globals.css'
import { AppProps } from 'next/app'
import { SpotifyAuthProvider } from '../lib/SpotifyAuth'

const App = ({ Component, pageProps }: AppProps) => {
  return (
    <SpotifyAuthProvider>
      <Component {...pageProps} />
    </SpotifyAuthProvider>
  )
}

export default App
