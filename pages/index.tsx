import Head from 'next/head'
import { useSession, signIn, signOut } from 'next-auth/client'
import SpotifyAnalysis from '../components/SpotifyAnalysis'
import { BASE_URL } from '../lib/constants'

const Home = () => {
  const [session, loadingSession] = useSession()

  return (
    <div>
      <Head>
        <title>Smoothify</title>
      </Head>
      <section>
        {session ? (
          <>
            <p>You&apos;re signed in!</p>
            <p>Name: {session.user?.name}</p>
            <SpotifyAnalysis />
            <button onClick={() => signOut({ callbackUrl: BASE_URL })}>
              Sign out
            </button>
          </>
        ) : (
          <button onClick={() => signIn('spotify')}>
            Sign in with Spotify
          </button>
        )}
      </section>
    </div>
  )
}

export default Home
