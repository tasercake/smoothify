from dataclasses import dataclass
from typing import List, Tuple

import numpy as np
from joblib import delayed
from scipy.spatial import cKDTree

from smoothify.optim.bottleneck_tsp.base import BottleneckTSPResult
from smoothify.utils.parallel import ProgressParallel


@dataclass
class KDTreeBottleneckTSPResult(BottleneckTSPResult):
    paths: List[List[int]]
    max_dists: List[float]
    best_path_idx: int


class KDTreeBottleneckTSP:
    def __init__(self, *, points: np.ndarray):
        self.points = points
        self.num_points = self.points.shape[0]

    def build_tree(self) -> cKDTree:
        tree = cKDTree(self.points)
        return tree

    def greedy_nearest_path(
        self, tree: cKDTree, source: int
    ) -> Tuple[List[int], float]:
        """
        Starting from the source node, construct a list of node indices by
        iteratively adding the nearest neighbor in the KD Tree.
        """
        current = source  # The node we're currently at
        visited = {source}  # Set of visited indices
        path = [source]  # The path so far (in order)
        max_dist = 0  # Length of the longest edge in the path

        # Loop until we've visited every point
        for i in range(self.num_points):
            # Start with the closest neighbor, keep incrementing until we find an unvisited neighbor
            for k in range(2, self.num_points):
                # Try to get the closest unvisited neighbor to the current node
                (distance,), (candidate_point,) = tree.query(
                    self.points[current], k=[k]
                )
                # Once an unvisited nearest neighbor is found
                if candidate_point not in visited:
                    current = candidate_point  # Move to the neighbor node
                    visited.add(candidate_point)  # Mark the neighbor node as visited
                    path.append(candidate_point)  # Add the neighbor node to the path
                    if distance > max_dist:
                        # Update the maximum edge length in the page
                        max_dist = distance
                    break
        return path, max_dist

    def get_best_path(self) -> KDTreeBottleneckTSPResult:
        tree = self.build_tree()
        tasks = ProgressParallel(n_jobs=-1, verbose=50)(
            [delayed(self.greedy_nearest_path)(tree, i) for i in range(self.num_points)]
        )
        paths, max_dists = zip(*tasks)
        best_path_idx = int(np.argmin(max_dists))
        best_path = paths[best_path_idx]
        min_max_dist = max_dists[best_path_idx]
        return KDTreeBottleneckTSPResult(
            paths=paths,
            max_dists=max_dists,
            best_path_idx=best_path_idx,
            best_path=best_path,
            min_max_dist=min_max_dist,
        )
