from typing import List

import pandas as pd
from tekore.model import AudioFeatures


class SpotifyFeatureConstructor:
    def __init__(self, audio_features_list: List[AudioFeatures]):
        self.audio_features_list = audio_features_list

    def construct_features(self, *, normalize: bool = True) -> pd.DataFrame:
        features_list = [
            {
                "danceability": audio_features.danceability,
                "energy": audio_features.energy,
                "loudness": audio_features.loudness,
                "speechiness": audio_features.speechiness,
                "acousticness": audio_features.acousticness,
                "instrumentalness": audio_features.instrumentalness,
                "liveness": audio_features.liveness,
                "valence": audio_features.valence,
                "tempo": audio_features.tempo,
            }
            for audio_features in self.audio_features_list
        ]

        # Convert to dataframe
        features_df = pd.DataFrame(features_list)

        # Normalize
        if normalize:
            features_df = self.normalize_features_df(features_df)
        return features_df

    @classmethod
    def normalize_features_df(cls, features_df):
        return (features_df - features_df.mean()) / features_df.std()
