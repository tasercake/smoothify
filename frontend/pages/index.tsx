import Head from 'next/head'
import { useEffect, useRef, useState } from 'react'

const generateRandomPoints = (nPoints: number, nDims: number) => {
  return Array.from({ length: nPoints }, () =>
    Array.from({ length: nDims }, () => Math.floor((Math.random() - 0.5) * 20))
  )
}

const N_POINTS = 10
const N_DIMS = 2

const Home = () => {
  const [points, setPoints] = useState<number[][] | undefined>()
  const [bestPath, setBestPath] = useState<number[] | undefined>(undefined)
  const workerRef = useRef<Worker>()

  useEffect(() => {
    const worker = new Worker(
      new URL('../lib/webWorkers/singleSourceBestPath.ts', import.meta.url)
    )
    workerRef.current = worker
    worker.onmessage = (event) => {
      console.debug('Main thread received event', event.data)
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
        <button
          onClick={() => {
            const points = generateRandomPoints(N_POINTS, N_DIMS)
            console.debug(`Generated ${points.length} random points`, points)
            setPoints(points)
            setBestPath(undefined)
          }}
        >
          Generate points
        </button>
        {points && (
          <>
            <p>Generated {points.length} points</p>
            <button onClick={() => workerRef.current?.postMessage(points)}>
              Sort points
            </button>
          </>
        )}
        {bestPath && (
          <>
            <p>Sorted {bestPath.length} points</p>
            {bestPath.join(', ')}
          </>
        )}
      </section>
    </div>
  )
}

export default Home
