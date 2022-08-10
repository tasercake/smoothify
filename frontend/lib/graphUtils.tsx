/**
 * Compute pairwise distances using geometric mean of distances along each coordinate
 */
export const distanceFn = (p1: number[], p2: number[]) => {
  // distance along each axis between each coordinate
  const diffs = p1.map((c, k) => Math.abs(c - p2[k]))
  // geometric mean of diffs
  return Math.pow(
    diffs.reduce((a, b) => a * b),
    1 / diffs.length
  )
}

export const singleSourceBestPath = (points: number[][]): number[] => {
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

  let currentIdx = Math.floor(Math.random() * points.length)
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

  return bestPath
}
