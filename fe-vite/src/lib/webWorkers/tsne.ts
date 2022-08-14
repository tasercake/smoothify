// @ts-ignore (tsne-js doesn't have types)
import TSNEjs from 'tsne-js'

/**
 * Simple tsne-js implementation
 */
const tsnejsImplementation = (points: number[][]) => {
  const model = new TSNEjs({
    dim: 2,
    perplexity: 32,
    earlyExaggeration: 4,
    learningRate: 256,
    nIter: 512,
    metric: 'euclidean'
  })
  model.init({
    data: points,
    type: 'dense'
  })
  model.run()
  return model.getOutputScaled()
}

/**
 * Webworker entry point
 */
onmessage = (event: MessageEvent) => {
  console.log('tsne worker received event', event)
  console.log('Computing tsne...')
  const points = event.data
  const output = tsnejsImplementation(points)
  console.log('tsne output', output)
  postMessage(output)
}
