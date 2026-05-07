pub(crate) const PACK_THREADS_ENV: &str = "PACK_THREADS";

pub(crate) fn worker_count(item_count: usize) -> usize {
    let requested = std::env::var(PACK_THREADS_ENV)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0);
    let available = std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(1);
    normalize_worker_count(requested, item_count, available)
}

fn normalize_worker_count(requested: Option<usize>, item_count: usize, available: usize) -> usize {
    if item_count <= 1 {
        return 1;
    }
    requested.unwrap_or(available.max(1)).clamp(1, item_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_count_caps_to_item_count() {
        assert_eq!(normalize_worker_count(Some(64), 3, 8), 3);
    }

    #[test]
    fn worker_count_can_force_single_thread() {
        assert_eq!(normalize_worker_count(Some(1), 100, 8), 1);
    }

    #[test]
    fn worker_count_defaults_to_available_parallelism() {
        assert_eq!(normalize_worker_count(None, 100, 8), 8);
    }
}
