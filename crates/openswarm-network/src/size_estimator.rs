//! Swarm size (N) estimation from Kademlia bucket density.
//!
//! Estimates the total number of agents in the network by analyzing
//! the distribution of peers across Kademlia routing table buckets.
//!
//! The algorithm works on the principle that in a Kademlia DHT with
//! uniformly distributed peer IDs, the density of peers in routing
//! table buckets follows a predictable distribution related to the
//! total network size.
//!
//! Specifically, for a node with peer ID P, the number of peers at
//! distance d (i.e., sharing a d-bit prefix) follows:
//!   E[bucket_d] = N / 2^(d+1)
//!
//! By observing the non-empty buckets closest to our ID, we can
//! estimate N by inverting this relationship.

use std::collections::VecDeque;

/// Estimates the total swarm size from Kademlia routing table observations.
///
/// Uses an exponentially weighted moving average (EWMA) to smooth
/// estimates across successive observations, preventing oscillation.
pub struct SwarmSizeEstimator {
    /// Recent estimates for smoothing.
    recent_estimates: VecDeque<u64>,
    /// Maximum number of recent estimates to keep.
    window_size: usize,
    /// Current smoothed estimate.
    current_estimate: u64,
    /// Minimum plausible network size (always at least 1: ourselves).
    min_size: u64,
}

impl SwarmSizeEstimator {
    /// Create a new estimator with the given smoothing window size.
    pub fn new(window_size: usize) -> Self {
        Self {
            recent_estimates: VecDeque::with_capacity(window_size),
            window_size,
            current_estimate: 1,
            min_size: 1,
        }
    }

    /// Update the estimate based on a snapshot of Kademlia bucket populations.
    ///
    /// `bucket_populations` is a slice where index `i` contains the number
    /// of peers in the bucket at distance `i` (i.e., peers sharing an
    /// `i`-bit prefix with our node ID).
    ///
    /// The estimation uses the non-empty bucket closest to our node ID
    /// (highest bucket index with peers) to compute: N = peers * 2^(index+1).
    /// Multiple non-empty buckets are combined for a weighted estimate.
    pub fn update_from_buckets(&mut self, bucket_populations: &[usize]) {
        let estimate = self.estimate_from_buckets(bucket_populations);

        self.recent_estimates.push_back(estimate);
        if self.recent_estimates.len() > self.window_size {
            self.recent_estimates.pop_front();
        }

        // Compute the median of recent estimates for robustness.
        self.current_estimate = self.compute_median();
    }

    /// Compute a raw estimate from bucket populations.
    ///
    /// For each non-empty bucket at index i, the local estimate is:
    ///   N_i = bucket_count * 2^(i+1)
    ///
    /// We take a weighted average where closer buckets (higher index)
    /// get more weight because they have more statistical power.
    fn estimate_from_buckets(&self, bucket_populations: &[usize]) -> u64 {
        let mut weighted_sum: f64 = 0.0;
        let mut total_weight: f64 = 0.0;

        for (i, &count) in bucket_populations.iter().enumerate() {
            if count == 0 {
                continue;
            }

            // Each peer in bucket i represents approximately 2^(i+1) total peers.
            let local_estimate = (count as f64) * 2.0_f64.powi((i as i32) + 1);

            // Weight by the bucket count: more peers = more reliable estimate.
            let weight = count as f64;
            weighted_sum += local_estimate * weight;
            total_weight += weight;
        }

        if total_weight < f64::EPSILON {
            return self.min_size;
        }

        let estimate = (weighted_sum / total_weight).round() as u64;
        estimate.max(self.min_size)
    }

    /// Compute the median of recent estimates.
    fn compute_median(&self) -> u64 {
        if self.recent_estimates.is_empty() {
            return self.min_size;
        }

        let mut sorted: Vec<u64> = self.recent_estimates.iter().copied().collect();
        sorted.sort_unstable();

        let mid = sorted.len() / 2;
        if sorted.len() % 2 == 0 && sorted.len() >= 2 {
            (sorted[mid - 1] + sorted[mid]) / 2
        } else {
            sorted[mid]
        }
    }

    /// Get the current smoothed estimate of the total swarm size.
    pub fn estimated_size(&self) -> u64 {
        self.current_estimate
    }

    /// Update estimate from a direct count of connected peers.
    ///
    /// This is a simple fallback when detailed bucket information
    /// is not available. It uses a multiplier based on the assumption
    /// that a typical node sees a logarithmic fraction of the network.
    pub fn update_from_peer_count(&mut self, connected_peers: usize) {
        // In a well-connected network, a node is typically connected to
        // O(k * log(N)) peers where k is the Kademlia replication factor (20).
        // We use a simple heuristic: N ~ connected_peers^1.5
        // This is rough but provides a reasonable lower bound.
        let estimate = if connected_peers <= 1 {
            connected_peers as u64
        } else {
            let n = connected_peers as f64;
            // Use the formula: N_est = n * ln(n) + n, clamped to min_size
            let est = n * n.ln() + n;
            (est.round() as u64).max(self.min_size)
        };

        self.recent_estimates.push_back(estimate);
        if self.recent_estimates.len() > self.window_size {
            self.recent_estimates.pop_front();
        }
        self.current_estimate = self.compute_median();
    }

    /// Reset the estimator, clearing all history.
    pub fn reset(&mut self) {
        self.recent_estimates.clear();
        self.current_estimate = self.min_size;
    }
}

impl Default for SwarmSizeEstimator {
    fn default() -> Self {
        Self::new(10)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_buckets() {
        let mut estimator = SwarmSizeEstimator::new(5);
        estimator.update_from_buckets(&[0, 0, 0, 0]);
        assert_eq!(estimator.estimated_size(), 1);
    }

    #[test]
    fn test_single_bucket() {
        let mut estimator = SwarmSizeEstimator::new(5);
        // 5 peers in bucket 3 → estimate = 5 * 2^4 = 80
        estimator.update_from_buckets(&[0, 0, 0, 5]);
        assert_eq!(estimator.estimated_size(), 80);
    }

    #[test]
    fn test_multiple_buckets() {
        let mut estimator = SwarmSizeEstimator::new(5);
        // Bucket 0: 1 peer → 1 * 2 = 2
        // Bucket 1: 2 peers → 2 * 4 = 8
        // Bucket 2: 3 peers → 3 * 8 = 24
        // Weighted avg = (2*1 + 8*2 + 24*3) / (1 + 2 + 3) = (2 + 16 + 72) / 6 = 15
        estimator.update_from_buckets(&[1, 2, 3]);
        assert_eq!(estimator.estimated_size(), 15);
    }

    #[test]
    fn test_smoothing_across_updates() {
        let mut estimator = SwarmSizeEstimator::new(3);
        estimator.update_from_buckets(&[0, 0, 0, 5]); // 80
        estimator.update_from_buckets(&[0, 0, 0, 10]); // 160
        estimator.update_from_buckets(&[0, 0, 0, 3]); // 48
        // Median of [80, 160, 48] → sort [48, 80, 160] → median = 80
        assert_eq!(estimator.estimated_size(), 80);
    }

    #[test]
    fn test_peer_count_fallback() {
        let mut estimator = SwarmSizeEstimator::new(5);
        estimator.update_from_peer_count(20);
        let est = estimator.estimated_size();
        // With 20 peers: 20 * ln(20) + 20 ≈ 20 * 3.0 + 20 = 80
        assert!(est > 10);
        assert!(est < 200);
    }
}
