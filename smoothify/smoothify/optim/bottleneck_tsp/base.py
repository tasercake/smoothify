from abc import ABC, abstractmethod
from dataclasses import dataclass
from typing import List


@dataclass
class BottleneckTSPResult:
    best_path: List[int]
    min_max_dist: float


class BottleneckTSP(ABC):
    @abstractmethod
    def get_best_path(self) -> BottleneckTSPResult:
        pass
