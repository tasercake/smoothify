//! Cost matrix and distance abstractions.

use ndarray::Array2;

/// A directed cost function: cost(from, to) may differ from cost(to, from).
pub trait CostFn {
    fn cost(&self, from: usize, to: usize) -> f64;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool { self.len() == 0 }
}

/// Dense asymmetric cost matrix stored as an NxN ndarray.
pub struct AsymmetricCostMatrix {
    matrix: Array2<f64>,
}

impl AsymmetricCostMatrix {
    pub fn new(matrix: Array2<f64>) -> Self {
        assert_eq!(matrix.nrows(), matrix.ncols());
        Self { matrix }
    }

    pub fn from_fn(n: usize, f: impl Fn(usize, usize) -> f64) -> Self {
        Self::new(Array2::from_shape_fn((n, n), |(i, j)| f(i, j)))
    }
}

impl CostFn for AsymmetricCostMatrix {
    fn cost(&self, from: usize, to: usize) -> f64 {
        self.matrix[[from, to]]
    }

    fn len(&self) -> usize {
        self.matrix.nrows()
    }
}
