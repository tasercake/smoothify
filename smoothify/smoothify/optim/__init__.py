"""
A collection of algorithms to find a smooth path through a set of high-dimensional points.
"""
from .base import Smoothifier, SmoothifyResult
from .kd_tree import KDTreeBottleneckTSP, KDTreeSmoothifyResult
