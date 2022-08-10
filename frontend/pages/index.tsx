import Head from 'next/head'
import { useEffect, useState } from 'react'

/**
 * Compute pairwise distances using geometric mean of distances along each coordinate
 */
const distanceFn = (p1: number[], p2: number[]) => {
  // distance along each axis between each coordinate
  const diffs = p1.map((c, k) => Math.abs(c - p2[k]))
  // geometric mean of diffs
  return Math.pow(
    diffs.reduce((a, b) => a * b),
    1 / diffs.length
  )
}

const getBestPath = (points: number[][]) => {
  const distances = points.map((p1, i) =>
    points.map((p2, j) => (i === j ? 0 : distanceFn(p1, p2)))
  )
  const distancesWithIndices = distances.map((row) =>
    row.map((d, i) => [i, d] as const)
  )
  const sortedDistances = distancesWithIndices.map((row) =>
    row.sort(([_, a], [__, b]) => a - b)
  )
  const nearestNeighbors = sortedDistances.map((row) =>
    row.slice(1).map(([i, _]) => i)
  )

  const visited = new Set<number>()
  const bestPath = []

  let currentIdx = 0 // TODO: Start with a random index
  visited.add(currentIdx)
  bestPath.push(currentIdx)

  while (visited.size < points.length) {
    const nextIdx = nearestNeighbors[currentIdx].find((i) => !visited.has(i))
    if (nextIdx === undefined) {
      break
    }
    visited.add(nextIdx)
    bestPath.push(nextIdx)
    currentIdx = nextIdx
  }

  return bestPath.map((i) => points[i])
}

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
            <button
              onClick={() => {
                setBestPath(getBestPath(points))
              }}
            >
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
