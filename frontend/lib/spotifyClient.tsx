import axios from 'axios'
import { SPOTIFY_API_BASE_URL } from './constants'

const spotifyClient = axios.create({ baseURL: SPOTIFY_API_BASE_URL })
export default spotifyClient
