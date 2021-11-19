from typing import List

import pandas as pd
from tekore.model import AudioFeatures


def construct_features(
    *, audio_features_list: List[AudioFeatures], normalize: bool = True
) -> pd.DataFrame:
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
        for audio_features in audio_features_list
    ]

    # Convert to dataframe
    features_df = pd.DataFrame(features_list)

    # Normalize
    if normalize:
        features_df = (features_df - features_df.mean()) / features_df.std()
    return features_df
