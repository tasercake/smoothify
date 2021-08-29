import Head from 'next/head'
import { GetStaticProps } from 'next'
import { useSession, signIn, signOut } from 'next-auth/client'
import { BASE_URL } from '../lib/constants'

const Home = () => {
  const [session, loading] = useSession()

  return (
    <div>
      <Head>
        <title>Title!</title>
      </Head>
      <section>
        {session ? (
          <>
            <p>You&apos;re signed in! </p>
            <p>Name: {session.user?.name}</p>
            <p>Email: {session.user?.email}</p>
            <button onClick={() => signOut({ callbackUrl: BASE_URL })}>
              Sign out
            </button>
          </>
        ) : (
          <button onClick={() => signIn('spotify')}>
            Sign in with Spotify
          </button>
          // <a href="/api/auth/signin">Sign in with Spotify</a>
        )}
        <p>[Your Self Introduction]</p>
        <p>
          (This is a sample website - you’ll be building a site like this in{' '}
          <a href="https://nextjs.org/learn">our Next.js tutorial</a>.)
        </p>
      </section>
      <section>
        <h2>Blog</h2>
      </section>
    </div>
  )
}

export const getStaticProps: GetStaticProps = async () => {
  return {
    props: {}
  }
}

export default Home
