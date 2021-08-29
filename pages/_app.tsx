import '../styles/globals.css'
import { AppProps } from 'next/app'
import { Provider as NextAuthProvider } from 'next-auth/client'
// import { SpotifyAuthProvider } from '../lib/SpotifyAuth'

const App = ({ Component, pageProps }: AppProps) => {
  return (
    <NextAuthProvider session={pageProps.session}>
      {/*<SpotifyAuthProvider>*/}
      <Component {...pageProps} />
      {/*</SpotifyAuthProvider>*/}
    </NextAuthProvider>
  )
}

export default App
