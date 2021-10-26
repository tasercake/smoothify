from typing import Any, List, Dict
from itertools import zip_longest

import pandas as pd

Track = Dict[str, Any]
AudioFeatures = Dict[str, Any]
AudioAnalysis = Dict[str, Any]


def construct_features(
    *,
    audio_features_list: List[AudioFeatures],
    audio_analysis_list: List[AudioAnalysis] = None,
    normalize: bool = True,
) -> pd.DataFrame:
    audio_analysis_list = audio_analysis_list or []
    features_list = []

    audio_features_keys = (
        "danceability",
        "energy",
        "loudness",
        "speechiness",
        "acousticness",
        "instrumentalness",
        "liveness",
        "valence",
        "tempo",
    )
    audio_analysis_track_keys = ("mode",)

    for audio_features, audio_analysis in zip_longest(
        audio_features_list, audio_analysis_list
    ):
        features = {}

        features.update({key: audio_features[key] for key in audio_features_keys})

        if audio_analysis:
            audio_analysis_track_features = audio_analysis["track"]
            features.update(
                {
                    key: audio_analysis_track_features[key]
                    for key in audio_analysis_track_keys
                }
            )

        features_list.append(features)

    # Convert to dataframe
    features_df = pd.DataFrame(features_list)

    # Normalize
    if normalize:
        features_df = (features_df - features_df.mean()) / features_df.std()
    return features_df
