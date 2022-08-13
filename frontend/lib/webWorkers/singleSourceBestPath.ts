import { greedyBestPath } from '../graphUtils'

onmessage = (event) => {
  console.log('singleSourceBestPath worker received event', event)
  console.log('Computing single source best path...')
  const path = greedyBestPath(event.data)
  console.log('Computed best path', path)
  postMessage(path)
}
