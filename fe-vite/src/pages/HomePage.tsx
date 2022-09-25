import React, { useEffect, useRef, useState } from 'react'
import { Link } from 'react-router-dom'
import Plot from 'react-plotly.js'
import * as tf from '@tensorflow/tfjs'
import BestPathWorker from '../lib/webWorkers/singleSourceBestPath?worker'
import TsneWorker from '../lib/webWorkers/tsne?worker'
import spotifyClient from '../lib/spotifyClient'

const generateRandomPoints = (nPoints: number, nDims: number) => {
  return Array.from({ length: nPoints }, () =>
    Array.from({ length: nDims }, () => Math.floor((Math.random() - 0.5) * 20))
  )
}

const getTrackFeatures = async (trackIds: string[]) => {
  const resp = await spotifyClient.get('/v1/audio-features', {
    params: {
      ids: trackIds.join(',')
    },
    headers: {
      Authorization: `Bearer ${localStorage.getItem('spotifyAuthToken')}`
    }
  })
  return resp.data.audio_features
}

const getLibrarySongs = async () => {
  const resp = await spotifyClient.get('/v1/me/tracks', {
    params: {
      limit: 50
    },
    headers: {
      Authorization: `Bearer ${localStorage.getItem('spotifyAuthToken')}`
    }
  })
  return resp.data.items
}

/**
 * Convert a Spotify track's features to a normalized feature vector.
 */
const featuresToArray = (features: any): number[] => {
  return [
    features.danceability,
    features.energy,
    features.loudness,
    features.speechiness,
    features.acousticness,
    features.instrumentalness,
    features.liveness,
    features.valence,
    features.tempo
  ]
}

/**
 * Convert a 2D array of Spotify features to a 2D matrix of normalized features.
 */
const featureListToMatrix = (featuresList: number[][]) => {
  const mat = tf.tensor(featuresList)
  // Normalize features such that each feature is in the range [0, 1]
  return mat.sub(mat.min()).div(mat.max())
}

const N_POINTS = 100
const N_DIMS = 9

const HomePage: React.FC = () => {
  const token = localStorage.getItem('spotifyAuthToken')

  const [songs, setSongs] = useState<any[]>([])

  const [points, setPoints] = useState<number[][] | undefined>()
  const [bestPath, setBestPath] = useState<number[] | undefined>(undefined)
  const [visPoints, setVisPoints] = useState<[number, number][] | undefined>(
    undefined
  )

  // Best path worker setup
  const bestPathWorkerRef = useRef<Worker>()
  useEffect(() => {
    const worker = new BestPathWorker()
    bestPathWorkerRef.current = worker
    worker.onmessage = (event) => {
      setBestPath(event.data)
    }
    return () => {
      // Terminate worker & set worker ref to undefined
      bestPathWorkerRef.current?.terminate()
      bestPathWorkerRef.current = undefined
    }
  }, [])

  // TSNE worker setup
  const tsneWorkerRef = useRef<Worker>()
  useEffect(() => {
    const worker = new TsneWorker()
    tsneWorkerRef.current = worker
    worker.onmessage = (event) => {
      setVisPoints(event.data)
    }
    return () => {
      // Terminate worker & set worker ref to undefined
      tsneWorkerRef.current?.terminate()
      tsneWorkerRef.current = undefined
    }
  })

  const authenticatedSection = (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
      <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
        <div>Logged in!</div>
        <Link to="/login">Login again</Link>
      </div>
      <button
        onClick={async () => {
          const songs = await getLibrarySongs()
          setSongs(songs)
          const features = await getTrackFeatures(
            songs.map((song: any) => song.track.id)
          )
          const points = featureListToMatrix(
            features.map(featuresToArray)
          ).arraySync() as number[][]
          console.log(points)
          setPoints(points)
        }}
      >
        Get songs
      </button>
      {songs && <p>Fetched {songs.length} songs</p>}
    </div>
  )

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
      <h1 className="text-3xl">Smoothify</h1>
      {token ? authenticatedSection : <Link to="/login">Login</Link>}
      <button
        className=""
        onClick={() => {
          const points = generateRandomPoints(N_POINTS, N_DIMS)
          setPoints(points)
          setBestPath(undefined)
        }}
      >
        Generate points
      </button>
      {points && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
          <p>{points.length} datapoints</p>
          <button
            onClick={() => {
              console.log('Sending tsne request', points)
              tsneWorkerRef.current?.postMessage(points)
            }}
          >
            TSNE
          </button>
          <button
            onClick={() => bestPathWorkerRef.current?.postMessage(points)}
          >
            Sort points
          </button>
        </div>
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
              marker: { color: 'red' },
              text: songs.map((song) => song.track.name)
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
    </div>
  )
}
export default HomePage
