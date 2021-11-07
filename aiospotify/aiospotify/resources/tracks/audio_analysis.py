from aiospotify.models.audio_analysis import SpotifyAudioAnalysis
from aiospotify.resources.abstract import GettableResource


class AudioAnalysis(GettableResource[SpotifyAudioAnalysis]):
    path = "/audio-analysis"
    response_type = SpotifyAudioAnalysis
