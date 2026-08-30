pub mod block_it;
pub mod block_it_for_bin;
pub mod champsim;
pub mod utils;

use log::debug;
use std::cmp::Reverse;
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::hash::Hash;

// use crate::block_it_for_bin::TraceEntry;

pub struct OptMissRatioResult {
    pub miss_ratio: f64,
    pub miss_count: usize,
    pub cache_accesses: usize,
    // pub miss_counts: Vec<usize>,
    pub hit_trace: Vec<bool>,
}

pub fn opt_miss_ratio(
    block_tags: &[u32],
    forward_refs: &[i32],
    block_count: usize,
    count_cold_as_hit: bool,
) -> OptMissRatioResult {
    assert_eq!(block_tags.len(), forward_refs.len());
    let cache_accesses = block_tags.len();
    let mut cache_misses = 0;
    let mut hit_trace = vec![false; block_tags.len()];
    let mut seen: HashSet<u32> = HashSet::new();

    // Use block_tag as key, value is Option<usize> (next use)
    let mut cache_map: HashMap<u32, Option<usize>> = HashMap::new();
    let mut reckoning: BTreeSet<(Reverse<usize>, u32)> = BTreeSet::new();

    for (i, (&tag, &forward_ri)) in block_tags.iter().zip(forward_refs.iter()).enumerate() {
        // Compute next use (as index), or usize::MAX if never used again
        let next_use = if forward_ri == i32::MAX {
            None
        } else {
            Some(i + forward_ri as usize)
        };
        let is_cold = !seen.contains(&tag);
        seen.insert(tag);

        if let Some(&prev_next_use) = cache_map.get(&tag) {
            // Cache hit
            reckoning.remove(&(Reverse(prev_next_use.unwrap_or(usize::MAX)), tag));
            cache_map.insert(tag, next_use);
            reckoning.insert((Reverse(next_use.unwrap_or(usize::MAX)), tag));
            hit_trace[i] = true;
        } else {
            // Miss (but maybe cold miss = hit)
            if is_cold && count_cold_as_hit {
                hit_trace[i] = true;
            } else {
                cache_misses += 1;
            }

            if cache_map.len() == block_count {
                // Evict element with furthest next use
                if let Some(evict_entry) = reckoning.iter().next().cloned() {
                    let (_, evict_tag) = evict_entry;
                    reckoning.remove(&evict_entry);
                    cache_map.remove(&evict_tag);
                }
            }
            cache_map.insert(tag, next_use);
            reckoning.insert((Reverse(next_use.unwrap_or(usize::MAX)), tag));
        }
    }

    OptMissRatioResult {
        miss_ratio: cache_misses as f64 / cache_accesses as f64,
        miss_count: cache_misses,
        cache_accesses,
        hit_trace,
    }
}

/// Generic version of the OPT miss ratio calculation
pub fn _opt_miss_ratio_for_any<T: PartialEq + Eq + Clone + Hash + Ord + Copy + std::fmt::Debug>(
    trace: &[T],
    block_count: usize,
    count_cold_as_hit: bool,
) -> OptMissRatioResult {
    let cache_accesses: usize = trace.len();
    let mut cache_misses: usize = 0;
    // let mut miss_counts: Vec<usize> = vec![0; trace.len()];
    let mut hit_trace: Vec<bool> = vec![false; trace.len()];
    let mut seen: HashSet<T> = HashSet::new();

    // Precompute next_use times for each element in the trace
    let mut next_use: Vec<Option<usize>> = vec![None; trace.len()];
    let mut last_seen: HashMap<&T, usize> = HashMap::new();
    for (i, element) in trace.iter().enumerate().rev() {
        if let Some(&next_i) = last_seen.get(element) {
            next_use[i] = Some(next_i);
        }
        last_seen.insert(element, i);
    }

    // Cache simulation using HashMap and BTreeSet
    let mut cache_map: HashMap<T, Option<usize>> = HashMap::new();
    let mut reckoning: BTreeSet<(Reverse<usize>, T)> = BTreeSet::new();

    for (i, element) in trace.iter().enumerate() {
        let next_time = next_use[i];
        let is_cold = !seen.contains(element);
        seen.insert(*element);

        debug!(
            "i: {}, Accessing: {:?}, next_use: {:?}",
            i, element, next_time
        );

        if let Some(&prev_next_time) = cache_map.get(element) {
            debug!("Cache hit: {:?}", element);
            reckoning.remove(&(Reverse(prev_next_time.unwrap()), *element));
            cache_map.insert(*element, next_time);
            reckoning.insert((Reverse(next_time.unwrap_or(usize::MAX)), *element));
            hit_trace[i] = true;
        } else {
            // Miss (but maybe cold miss = hit)
            if is_cold && count_cold_as_hit {
                hit_trace[i] = true;
            } else {
                cache_misses += 1;
            }
            // reckoning.insert((Reverse(next_time.unwrap_or(usize::MAX)), element.clone()));

            if cache_map.len() == block_count {
                // Evict element with the furthest next use
                let evict_entry = *reckoning.iter().next().unwrap();
                let (_, evict_element) = evict_entry;
                reckoning.remove(&evict_entry);
                if cache_map.remove(&evict_element).is_some() {
                    // if found in cache, it means we evicted an element in cache with the new one that has shorter reuse.
                    debug!("Found in cache_map: {:?}", evict_element);
                    cache_map.insert(*element, next_time);
                    reckoning.insert((Reverse(next_time.unwrap_or(usize::MAX)), *element));
                } else {
                    panic!("item should be in cache")
                }
            } else {
                cache_map.insert(*element, next_time);
                reckoning.insert((Reverse(next_time.unwrap_or(usize::MAX)), *element));
            }
        }
    }

    OptMissRatioResult {
        miss_ratio: cache_misses as f64 / cache_accesses as f64,
        miss_count: cache_misses,
        cache_accesses,
        hit_trace,
    }
}

pub fn opt_stack_miss_ratio(block_tags: &[u32], block_count: usize) -> OptMissRatioResult {
    let mut stack: VecDeque<u32> = VecDeque::new();
    let mut histogram: HashMap<usize, usize> = HashMap::new();
    let mut hit_trace = vec![false; block_tags.len()];

    for (i, &tag) in block_tags.iter().enumerate() {
        if let Some(pos) = stack.iter().position(|&x| x == tag) {
            // Seen before (reuse)
            *histogram.entry(pos).or_insert(0) += 1;
            hit_trace[i] = pos < block_count;
            stack.remove(pos);
        } else {
            // Cold miss
            *histogram.entry(usize::MAX).or_insert(0) += 1;
        }
        stack.push_front(tag);
    }

    // Count total misses for given cache size
    let total_accesses = block_tags.len();
    let mut total_misses = 0;
    for (dist, count) in &histogram {
        if *dist > block_count {
            total_misses += *count;
        }
    }

    OptMissRatioResult {
        miss_ratio: total_misses as f64 / total_accesses as f64,
        miss_count: total_misses,
        cache_accesses: total_accesses,
        hit_trace,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_test1() {
        let trace = vec![0, 1, 2, 0, 3, 0, 4, 2, 3, 0, 3, 2, 1, 2, 0, 1, 0, 1, 5, 7];

        // let result = opt_miss_ratio_old(&trace, 3);
        // println!("Miss ratio: {}", result);

        let result = _opt_miss_ratio_for_any(&trace, 4, false);
        println!("Miss ratio: {}", result.miss_ratio);
        assert_eq!(result.miss_ratio, 0.4);
    }

    #[test]
    fn simple_test2() {
        let trace = vec![1, 2, 3, 4, 1, 2, 3, 3, 1, 2, 3, 4, 1, 2, 3, 4];
        let result = _opt_miss_ratio_for_any(&trace, 4, false);
        println!("Miss ratio: {}", result.miss_ratio);
        assert_eq!(result.miss_ratio, 0.25);
    }

    #[test]
    fn simple_test3() {
        let trace = vec![2, 3, 4, 2, 1, 3, 7, 5, 4, 3];
        let result = _opt_miss_ratio_for_any(&trace, 2, false);
        println!("Miss ratio: {}", result.miss_ratio);
        assert_eq!(result.miss_ratio, 0.8);
    }

    #[test]
    fn simple_test4() {
        let trace = vec![
            7, 1, 2, 3, 5, 4, 1, 2, 2, 1, 3, 4, 5, 1, 6, 7, 1, 2, 3, 5, 4, 1, 2, 2, 1, 3, 4, 5, 1,
            6,
        ];
        let result = _opt_miss_ratio_for_any(&trace, 2, false);
        println!("Miss ratio: {}", result.miss_ratio);
        assert_eq!(result.miss_ratio, 0.7);
    }

    // #[test]
    // fn opt_miss_ratio_test() {
    //     let mut rng = thread_rng();
    //     let trace: Vec<usize> = (0..1024).map(|_| rng.gen_range(0..256)).collect();

    //     // // let start = Instant::now();
    //     // let result = opt_miss_ratio_old(&trace, 128);
    //     // // let duration = start.elapsed();
    //     // println!("Optimal miss ratio: {}", result);
    //     // // println!("Time taken: {:?}", duration);

    //     // let start = Instant::now();
    //     let result = _opt_miss_ratio_for_any(&trace, 128, false);
    //     // let duration = start.elapsed();
    //     println!("Optimal miss ratio: {}", result.miss_ratio);
    //     // println!("Time taken: {:?}", duration);
    //     assert!(result.miss_ratio < 0.5);
    // }
}
