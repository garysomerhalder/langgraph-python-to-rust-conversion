//! SIMD-optimized mathematical and batch operations for high-performance computing.
//!
//! This module provides vectorized implementations of common mathematical operations
//! that can process multiple values simultaneously using SIMD instructions.

use wide::f64x4;

/// SIMD-optimized mathematical operations for batch processing
pub struct SimdMath;

impl SimdMath {
    /// Vectorized addition for arrays of f64 values
    ///
    /// # Arguments
    /// * `values` - Array of values to sum
    ///
    /// # Returns
    /// Sum of all values, computed using SIMD when possible
    pub fn vector_sum(values: &[f64]) -> f64 {
        let chunks = values.chunks_exact(4);
        let remainder = chunks.remainder();

        // Process 4 elements at a time using SIMD
        let simd_sum = chunks
            .map(|chunk| f64x4::from([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .fold(f64x4::splat(0.0), |acc, v| acc + v);

        // Manual horizontal sum for f64x4
        let array = simd_sum.to_array();
        let horizontal_sum = array[0] + array[1] + array[2] + array[3];

        // Handle remaining elements with scalar operations
        let scalar_sum: f64 = remainder.iter().sum();

        horizontal_sum + scalar_sum
    }

    /// Vectorized multiplication for arrays of f64 values
    ///
    /// # Arguments
    /// * `values` - Array of values to multiply
    ///
    /// # Returns
    /// Product of all values, computed using SIMD when possible
    pub fn vector_product(values: &[f64]) -> f64 {
        if values.is_empty() {
            return 1.0;
        }

        let chunks = values.chunks_exact(4);
        let remainder = chunks.remainder();

        // Process 4 elements at a time using SIMD
        let simd_product = chunks
            .map(|chunk| f64x4::from([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .fold(f64x4::splat(1.0), |acc, v| acc * v);

        // Manual horizontal product for f64x4
        let array = simd_product.to_array();
        let horizontal_product = array[0] * array[1] * array[2] * array[3];

        // Handle remaining elements with scalar operations
        let scalar_product: f64 = remainder.iter().product();

        horizontal_product * scalar_product
    }

    /// Vectorized element-wise operations between two arrays
    ///
    /// # Arguments
    /// * `a` - First array
    /// * `b` - Second array
    /// * `op` - Operation to perform (Add, Subtract, Multiply, Divide)
    ///
    /// # Returns
    /// Result array with element-wise operations applied
    pub fn vector_elementwise_op(a: &[f64], b: &[f64], op: VectorOp) -> Vec<f64> {
        let len = a.len().min(b.len());
        let mut result = Vec::with_capacity(len);

        let chunks_a = a[..len].chunks_exact(4);
        let chunks_b = b[..len].chunks_exact(4);
        let remainder_a = chunks_a.remainder();
        let remainder_b = chunks_b.remainder();

        // Process 4 elements at a time using SIMD
        for (chunk_a, chunk_b) in chunks_a.zip(chunks_b) {
            let vec_a = f64x4::from([chunk_a[0], chunk_a[1], chunk_a[2], chunk_a[3]]);
            let vec_b = f64x4::from([chunk_b[0], chunk_b[1], chunk_b[2], chunk_b[3]]);

            let vec_result = match op {
                VectorOp::Add => vec_a + vec_b,
                VectorOp::Subtract => vec_a - vec_b,
                VectorOp::Multiply => vec_a * vec_b,
                VectorOp::Divide => vec_a / vec_b,
            };

            result.extend_from_slice(&vec_result.to_array());
        }

        // Handle remaining elements with scalar operations
        for (a_val, b_val) in remainder_a.iter().zip(remainder_b.iter()) {
            let scalar_result = match op {
                VectorOp::Add => a_val + b_val,
                VectorOp::Subtract => a_val - b_val,
                VectorOp::Multiply => a_val * b_val,
                VectorOp::Divide => a_val / b_val,
            };
            result.push(scalar_result);
        }

        result
    }

    /// Vectorized square root computation
    ///
    /// # Arguments
    /// * `values` - Array of values to compute square root for
    ///
    /// # Returns
    /// Array of square root values
    pub fn vector_sqrt(values: &[f64]) -> Vec<f64> {
        let mut result = Vec::with_capacity(values.len());
        let chunks = values.chunks_exact(4);
        let remainder = chunks.remainder();

        // Process 4 elements at a time using SIMD
        for chunk in chunks {
            let vec = f64x4::from([chunk[0], chunk[1], chunk[2], chunk[3]]);
            let sqrt_vec = vec.sqrt();
            result.extend_from_slice(&sqrt_vec.to_array());
        }

        // Handle remaining elements with scalar operations
        for &value in remainder {
            result.push(value.sqrt());
        }

        result
    }

    /// Vectorized power computation
    ///
    /// # Arguments
    /// * `bases` - Array of base values
    /// * `exponent` - Single exponent to apply to all bases
    ///
    /// # Returns
    /// Array of power results
    pub fn vector_power(bases: &[f64], exponent: f64) -> Vec<f64> {
        let mut result = Vec::with_capacity(bases.len());
        let chunks = bases.chunks_exact(4);
        let remainder = chunks.remainder();

        // Process 4 elements at a time using SIMD
        for chunk in chunks {
            // Apply power operation element-wise
            let pow_array = [
                chunk[0].powf(exponent),
                chunk[1].powf(exponent),
                chunk[2].powf(exponent),
                chunk[3].powf(exponent),
            ];
            result.extend_from_slice(&pow_array);
        }

        // Handle remaining elements with scalar operations
        for &base in remainder {
            result.push(base.powf(exponent));
        }

        result
    }
}

/// Supported vectorized operations
#[derive(Debug, Clone, Copy)]
pub enum VectorOp {
    /// Element-wise addition
    Add,
    /// Element-wise subtraction
    Subtract,
    /// Element-wise multiplication
    Multiply,
    /// Element-wise division
    Divide,
}

/// SIMD-optimized batch operations for state processing
pub struct SimdBatch;

impl SimdBatch {
    /// Process multiple state updates in parallel using SIMD
    ///
    /// # Arguments
    /// * `values` - Array of numeric values to process
    /// * `operation` - Batch operation to apply
    ///
    /// # Returns
    /// Processed values array
    pub fn batch_process_values(values: &[f64], operation: BatchOp) -> Vec<f64> {
        match operation {
            BatchOp::Normalize => Self::batch_normalize(values),
            BatchOp::Scale(factor) => Self::batch_scale(values, factor),
            BatchOp::Clamp(min, max) => Self::batch_clamp(values, min, max),
        }
    }

    /// Normalize values to [0, 1] range using SIMD
    fn batch_normalize(values: &[f64]) -> Vec<f64> {
        if values.is_empty() {
            return Vec::new();
        }

        let min_val = values.iter().copied().fold(f64::INFINITY, f64::min);
        let max_val = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let range = max_val - min_val;

        if range == 0.0 {
            return vec![0.0; values.len()];
        }

        let mut result = Vec::with_capacity(values.len());
        let chunks = values.chunks_exact(4);
        let remainder = chunks.remainder();

        let min_vec = f64x4::splat(min_val);
        let range_vec = f64x4::splat(range);

        // Process 4 elements at a time using SIMD
        for chunk in chunks {
            let val_vec = f64x4::from([chunk[0], chunk[1], chunk[2], chunk[3]]);
            let normalized = (val_vec - min_vec) / range_vec;
            result.extend_from_slice(&normalized.to_array());
        }

        // Handle remaining elements
        for &value in remainder {
            result.push((value - min_val) / range);
        }

        result
    }

    /// Scale values by a factor using SIMD
    fn batch_scale(values: &[f64], factor: f64) -> Vec<f64> {
        let mut result = Vec::with_capacity(values.len());
        let chunks = values.chunks_exact(4);
        let remainder = chunks.remainder();
        let factor_vec = f64x4::splat(factor);

        // Process 4 elements at a time using SIMD
        for chunk in chunks {
            let val_vec = f64x4::from([chunk[0], chunk[1], chunk[2], chunk[3]]);
            let scaled = val_vec * factor_vec;
            result.extend_from_slice(&scaled.to_array());
        }

        // Handle remaining elements
        for &value in remainder {
            result.push(value * factor);
        }

        result
    }

    /// Clamp values to a range using SIMD
    fn batch_clamp(values: &[f64], min_val: f64, max_val: f64) -> Vec<f64> {
        let mut result = Vec::with_capacity(values.len());
        let chunks = values.chunks_exact(4);
        let remainder = chunks.remainder();

        let min_vec = f64x4::splat(min_val);
        let max_vec = f64x4::splat(max_val);

        // Process 4 elements at a time using SIMD
        for chunk in chunks {
            let val_vec = f64x4::from([chunk[0], chunk[1], chunk[2], chunk[3]]);
            let clamped = val_vec.max(min_vec).min(max_vec);
            result.extend_from_slice(&clamped.to_array());
        }

        // Handle remaining elements
        for &value in remainder {
            result.push(value.max(min_val).min(max_val));
        }

        result
    }
}

/// Batch processing operations
#[derive(Debug, Clone, Copy)]
pub enum BatchOp {
    /// Normalize values to [0, 1] range
    Normalize,
    /// Scale values by a factor
    Scale(f64),
    /// Clamp values to a range
    Clamp(f64, f64),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simd_vector_sum() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];
        let sum = SimdMath::vector_sum(&values);
        assert_eq!(sum, 45.0);
    }

    #[test]
    fn test_simd_vector_product() {
        let values = vec![2.0, 3.0, 4.0];
        let product = SimdMath::vector_product(&values);
        assert_eq!(product, 24.0);
    }

    #[test]
    fn test_simd_elementwise_operations() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let b = vec![2.0, 3.0, 4.0, 5.0, 6.0];

        let add_result = SimdMath::vector_elementwise_op(&a, &b, VectorOp::Add);
        assert_eq!(add_result, vec![3.0, 5.0, 7.0, 9.0, 11.0]);

        let mult_result = SimdMath::vector_elementwise_op(&a, &b, VectorOp::Multiply);
        assert_eq!(mult_result, vec![2.0, 6.0, 12.0, 20.0, 30.0]);
    }

    #[test]
    fn test_batch_normalize() {
        let values = vec![0.0, 5.0, 10.0];
        let normalized = SimdBatch::batch_process_values(&values, BatchOp::Normalize);
        assert_eq!(normalized, vec![0.0, 0.5, 1.0]);
    }

    #[test]
    fn test_batch_scale() {
        let values = vec![1.0, 2.0, 3.0, 4.0];
        let scaled = SimdBatch::batch_process_values(&values, BatchOp::Scale(2.0));
        assert_eq!(scaled, vec![2.0, 4.0, 6.0, 8.0]);
    }

    #[test]
    fn test_batch_clamp() {
        let values = vec![-1.0, 0.5, 1.5, 3.0];
        let clamped = SimdBatch::batch_process_values(&values, BatchOp::Clamp(0.0, 2.0));
        assert_eq!(clamped, vec![0.0, 0.5, 1.5, 2.0]);
    }
}
