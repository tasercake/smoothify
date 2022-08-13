// @ts-ignore (tsne-js doesn't have types)
import TSNE from 'tsne-js'

onmessage = (event) => {
  console.log('tsne worker received event', event)
  console.log('Computing tsne...')
  const points = event.data
  const model = new TSNE({
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
  console.log('tsne run finished')
  const output = model.getOutputScaled()
  console.log('tsne output', output)
  postMessage(output)
}
