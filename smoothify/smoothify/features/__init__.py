from typing import Any, List, Dict
import pandas as pd

Track = Dict[str, Any]
AudioFeatures = Dict[str, Any]
AudioAnalysis = Dict[str, Any]


def construct_features(
    *,
    track_list: List[Track],
    audio_features_list: List[AudioFeatures],
    audio_analysis_list: List[AudioAnalysis],
    normalize: bool = True,
) -> pd.DataFrame:
    features_list = []

    if not len(track_list) == len(audio_features_list) == len(audio_analysis_list):
        raise ValueError("Sequence lengths must match.")

    track_keys = ("popularity",)
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

    for track, audio_features, audio_analysis in zip(
        track_list, audio_features_list, audio_analysis_list
    ):
        features = {}
        features.update({key: track[key] for key in track_keys})

        features.update({key: audio_features[key] for key in audio_features_keys})

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
        features_df = (features_df - features_df.min()) / (
            features_df.max() - features_df.min()
        )
    return features_df
