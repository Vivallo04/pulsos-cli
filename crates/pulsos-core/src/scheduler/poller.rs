use std::future::Future;
use std::time::Duration;

/// Default batch size for staggered fetching (TDD §5.5).
pub const BATCH_SIZE: usize = 5;

/// Default inter-batch delay in seconds (TDD §5.5).
pub const BATCH_DELAY_SECS: u64 = 7;

/// Process `items` in chunks of `batch_size`, applying `f` to each item
/// and collecting all results. Sleeps `delay_secs` between batches
/// (not after the last batch).
pub async fn stagger<'items, T, F, Fut, R>(
    items: &'items [T],
    batch_size: usize,
    delay_secs: u64,
    f: F,
) -> Vec<R>
where
    T: Sync,
    F: Fn(&'items T) -> Fut,
    Fut: Future<Output = R>,
{
    let mut results = Vec::with_capacity(items.len());
    let chunks: Vec<&[T]> = items.chunks(batch_size).collect();
    let num_chunks = chunks.len();

    for (i, chunk) in chunks.into_iter().enumerate() {
        for item in chunk {
            results.push(f(item).await);
        }
        if i < num_chunks - 1 {
            tokio::time::sleep(Duration::from_secs(delay_secs)).await;
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stagger_empty() {
        let items: Vec<u32> = vec![];
        let results = stagger(&items, BATCH_SIZE, BATCH_DELAY_SECS, |x| {
            std::future::ready(*x)
        })
        .await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn stagger_single_batch_no_delay() {
        // 3 items with batch_size=5: all in one batch, no sleep
        let items = vec![1u32, 2, 3];
        let results = stagger(&items, 5, 0, |x| std::future::ready(*x * 2)).await;
        assert_eq!(results, vec![2, 4, 6]);
    }

    #[tokio::test]
    async fn stagger_multiple_batches() {
        // 7 items, batch_size=3 → 3 batches; delay=0 so test runs fast
        let items: Vec<u32> = (1..=7).collect();
        let results = stagger(&items, 3, 0, |x| std::future::ready(*x)).await;
        assert_eq!(results, vec![1, 2, 3, 4, 5, 6, 7]);
    }
}
