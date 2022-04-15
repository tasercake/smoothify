from abc import ABC, abstractmethod
from dataclasses import dataclass
from typing import List

import numpy as np


@dataclass
class SmoothifyResult:
    best_path: List[int]
    min_max_dist: float


class Smoothifier(ABC):
    @abstractmethod
    def get_best_path(self, *, points: np.ndarray) -> SmoothifyResult:
        """
        Find the 'optimal' path through the given points in N-dimensional space.
        The heuristic that determines the optimal path is up to the implementation.

        Args:
            points: A KxN array of K points in N-dimensional space.

        Returns:
            A SmoothifyResult object containing the 'best' path through the points
            based on the algorithm's heuristic.
        """
        pass
