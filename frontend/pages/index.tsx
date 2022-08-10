import Head from 'next/head'
import { useEffect, useRef, useState } from 'react'

const generateRandomPoints = (nPoints: number, nDims: number) => {
  return Array.from({ length: nPoints }, () =>
    Array.from({ length: nDims }, () => Math.floor((Math.random() - 0.5) * 20))
  )
}

const N_POINTS = 1000
const N_DIMS = 20

const Home = () => {
  const [points, setPoints] = useState<number[][] | undefined>()
  const [bestPath, setBestPath] = useState<number[][] | undefined>(undefined)
  const workerRef = useRef<Worker>()

  useEffect(() => {
    const worker = new Worker(
      new URL('../lib/webWorkers/singleSourceBestPath.ts', import.meta.url)
    )
    workerRef.current = worker
    worker.onmessage = (event) => {
      console.log('Main thread received event', event.data)
      setBestPath(event.data)
    }
  }, [])

  return (
    <div>
      <Head>
        <title>Smoothify</title>
      </Head>
      <section>
        <h1>Smoothify</h1>
        <p>Original</p>
        <button
          onClick={() => {
            setPoints(generateRandomPoints(N_POINTS, N_DIMS))
            setBestPath(undefined)
          }}
        >
          Generate points
        </button>
        {points && (
          <>
            <ol>
              {points.map((point) => (
                <li key={point.join(' ')}>{point.join(' ')}</li>
              ))}
            </ol>
            <button onClick={() => workerRef.current?.postMessage(points)}>
              Sort points
            </button>
            <p>Sorted</p>
            <ol>
              {bestPath &&
                bestPath.map((idx) => (
                  <li key={idx.join(' ')}>{idx.join(' ')}</li>
                ))}
            </ol>
          </>
        )}
      </section>
    </div>
  )
}

export default Home
