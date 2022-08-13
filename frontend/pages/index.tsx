import Head from 'next/head'
import React, { useEffect, useRef, useState } from 'react'
import dynamic from 'next/dynamic'

/**
 * No-SSR version of the plotly Plot component
 */
const Plot = dynamic(() => import('react-plotly.js'), {
  ssr: false
})

const generateRandomPoints = (nPoints: number, nDims: number) => {
  return Array.from({ length: nPoints }, () =>
    Array.from({ length: nDims }, () => Math.floor((Math.random() - 0.5) * 20))
  )
}

const N_POINTS = 100
const N_DIMS = 9

const Home = () => {
  const [points, setPoints] = useState<number[][] | undefined>()
  const [bestPath, setBestPath] = useState<number[] | undefined>(undefined)
  const [visPoints, setVisPoints] = useState<[number, number][] | undefined>(
    undefined
  )

  // Best path worker setup
  const bestPathWorkerRef = useRef<Worker>()
  useEffect(() => {
    const worker = new Worker(
      new URL('../lib/webWorkers/singleSourceBestPath.ts', import.meta.url)
    )
    bestPathWorkerRef.current = worker
    worker.onmessage = (event) => {
      setBestPath(event.data)
    }
  }, [])

  // TSNE worker setup
  const tsneWorkerRef = useRef<Worker>()
  useEffect(() => {
    const worker = new Worker(
      new URL('../lib/webWorkers/tsne.ts', import.meta.url)
    )
    tsneWorkerRef.current = worker
    worker.onmessage = (event) => {
      setVisPoints(event.data)
    }
  })

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
            console.log(`Generated ${points.length} random points`, points)
            setPoints(points)
            setBestPath(undefined)
          }}
        >
          Generate points
        </button>
        {points && (
          <>
            <p>Generated {points.length} points</p>
            <button onClick={() => tsneWorkerRef.current?.postMessage(points)}>
              TSNE
            </button>
            <button
              onClick={() => bestPathWorkerRef.current?.postMessage(points)}
            >
              Sort points
            </button>
          </>
        )}
        <br />
        {visPoints && (
          <Plot
            data={[
              {
                x: visPoints.map(([x, _]) => x),
                y: visPoints.map(([_, y]) => y),
                type: 'scattergl',
                mode: 'markers',
                marker: { color: 'red' }
              }
            ]}
            layout={{ width: 400, height: 400 }}
          />
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
