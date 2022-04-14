from abc import ABC, abstractmethod
from dataclasses import dataclass
from typing import List

import numpy as np


@dataclass
class SmoothifyResult:
    best_path: List[int]
    min_max_dist: float


class Smoothifier(ABC):
    def __init__(self, *, points: np.ndarray):
        self.points = points
        self.num_points = self.points.shape[0]

    @abstractmethod
    def get_best_path(self) -> SmoothifyResult:
        pass
