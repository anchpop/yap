//! Deterministic random sampling library for filtering large collections to a target size.
//!
//! This library provides functionality to reduce a large collection of items to a target count
//! using deterministic pseudo-random sampling. The sampling is deterministic based on item content,
//! meaning the same items will always be kept or filtered consistently.
//!
//! # Example
//!
//! ```
//! use sentence_sampler::sample_to_target;
//!
//! let items = vec!["sentence1", "sentence2", "sentence3", "sentence4", "sentence5"];
//! let target_count = 3;
//!
//! let sampled = sample_to_target(items, target_count, |item| item.to_string());
//! assert!(sampled.len() <= target_count + 1); // Approximately target_count
//! ```

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use rand::RngExt;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

/// Sample a collection to approximately reach a target count using deterministic random sampling.
///
/// If the input collection has fewer items than the target, all items are returned.
/// If it has more, items are randomly filtered using a deterministic seed based on their content.
///
/// # Arguments
///
/// * `items` - The collection to sample from
/// * `target_count` - The desired approximate number of items in the output
/// * `key_fn` - A function that extracts a hashable key from each item for deterministic seeding
///
/// # Returns
///
/// A vector containing approximately `target_count` items (may be slightly more or less due to
/// probabilistic sampling).
///
/// # Example
///
/// ```
/// use sentence_sampler::sample_to_target;
///
/// struct SentencePair {
///     target: String,
///     native: String,
/// }
///
/// let pairs = vec![
///     SentencePair { target: "Hello".to_string(), native: "Bonjour".to_string() },
///     SentencePair { target: "Goodbye".to_string(), native: "Au revoir".to_string() },
/// ];
///
/// let sampled = sample_to_target(pairs, 1, |pair| (pair.target.clone(), pair.native.clone()));
/// ```
pub fn sample_to_target<T, K, F>(items: Vec<T>, target_count: usize, key_fn: F) -> Vec<T>
where
    K: Hash,
    F: Fn(&T) -> K,
{
    if items.len() <= target_count {
        return items;
    }

    // Calculate the probability of keeping each item
    let keep_probability = target_count as f64 / items.len() as f64;

    items
        .into_iter()
        .filter(|item| {
            // Create a deterministic seed based on the item's key
            let mut hasher = DefaultHasher::new();
            key_fn(item).hash(&mut hasher);
            let seed = hasher.finish();

            // Create RNG with this seed
            let mut rng = ChaCha8Rng::seed_from_u64(seed);

            // Keep this item with probability keep_probability
            rng.random::<f64>() < keep_probability
        })
        .collect()
}

/// Sample a collection with detailed logging, returning both the sampled items and statistics.
///
/// This is similar to `sample_to_target` but provides information about the sampling process,
/// useful for debugging or when you need to report the sampling results.
///
/// # Arguments
///
/// * `items` - The collection to sample from
/// * `target_count` - The desired approximate number of items in the output
/// * `key_fn` - A function that extracts a hashable key from each item for deterministic seeding
///
/// # Returns
///
/// A tuple containing:
/// - The sampled vector
/// - `SamplingStats` with information about the sampling process
pub fn sample_to_target_with_stats<T, K, F>(
    items: Vec<T>,
    target_count: usize,
    key_fn: F,
) -> (Vec<T>, SamplingStats)
where
    K: Hash,
    F: Fn(&T) -> K,
{
    let original_count = items.len();

    if items.len() <= target_count {
        return (
            items,
            SamplingStats {
                original_count,
                target_count,
                final_count: original_count,
                was_sampled: false,
            },
        );
    }

    let keep_probability = target_count as f64 / items.len() as f64;

    let sampled: Vec<T> = items
        .into_iter()
        .filter(|item| {
            let mut hasher = DefaultHasher::new();
            key_fn(item).hash(&mut hasher);
            let seed = hasher.finish();
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            rng.random::<f64>() < keep_probability
        })
        .collect();

    let final_count = sampled.len();

    (
        sampled,
        SamplingStats {
            original_count,
            target_count,
            final_count,
            was_sampled: true,
        },
    )
}

/// Statistics about the sampling process
#[derive(Debug, Clone, Copy)]
pub struct SamplingStats {
    /// The number of items before sampling
    pub original_count: usize,
    /// The target count requested
    pub target_count: usize,
    /// The actual number of items after sampling
    pub final_count: usize,
    /// Whether sampling was actually performed (false if original_count <= target_count)
    pub was_sampled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sample_smaller_than_target() {
        let items = vec!["a", "b", "c"];
        let result = sample_to_target(items.clone(), 10, |s| s.to_string());
        assert_eq!(result.len(), 3);
        assert_eq!(result, items);
    }

    #[test]
    fn test_sample_deterministic() {
        let items: Vec<String> = (0..1000).map(|i| format!("item_{i}")).collect();
        let target = 100;

        let result1 = sample_to_target(items.clone(), target, |s| s.clone());
        let result2 = sample_to_target(items.clone(), target, |s| s.clone());

        // Same items should be sampled both times
        assert_eq!(result1, result2);
    }

    #[test]
    fn test_sample_approximate_target() {
        let items: Vec<String> = (0..10000).map(|i| format!("item_{i}")).collect();
        let target = 1000;

        let result = sample_to_target(items, target, |s| s.clone());

        // Should be approximately the target (within 20%)
        let lower_bound = (target as f64 * 0.8) as usize;
        let upper_bound = (target as f64 * 1.2) as usize;
        assert!(
            result.len() >= lower_bound && result.len() <= upper_bound,
            "Result length {} not within bounds [{}, {}]",
            result.len(),
            lower_bound,
            upper_bound
        );
    }

    #[test]
    fn test_sample_with_stats() {
        let items: Vec<String> = (0..1000).map(|i| format!("item_{i}")).collect();
        let target = 100;

        let (result, stats) = sample_to_target_with_stats(items, target, |s| s.clone());

        assert_eq!(stats.original_count, 1000);
        assert_eq!(stats.target_count, 100);
        assert_eq!(stats.final_count, result.len());
        assert!(stats.was_sampled);
    }

    #[test]
    fn test_stats_no_sampling_needed() {
        let items = vec!["a", "b", "c"];
        let target = 10;

        let (result, stats) = sample_to_target_with_stats(items, target, |s| s.to_string());

        assert_eq!(stats.original_count, 3);
        assert_eq!(stats.target_count, 10);
        assert_eq!(stats.final_count, 3);
        assert!(!stats.was_sampled);
        assert_eq!(result.len(), 3);
    }
}
