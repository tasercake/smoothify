from aiospotify.models.audio_features import SpotifyAudioFeatures
from aiospotify.resources.abstract import GettableResource


class AudioFeatures(GettableResource[SpotifyAudioFeatures]):
    path = "/audio-features"
    response_type = SpotifyAudioFeatures
