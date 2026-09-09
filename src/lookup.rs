//! Bounded advisory matching. Never use fuzzy matches to select a mutation.
/// Optimal string alignment distance, bounded to small catalog names.
pub(crate) fn close_distance(query: &[char], candidate: &str, threshold: usize) -> Option<usize> {
    if query.len() > 64 {
        return None;
    }
    let candidate: Vec<_> = candidate.chars().take(65).collect();
    if candidate.len() > 64 || query.len().abs_diff(candidate.len()) > threshold {
        return None;
    }
    let mut distance = vec![vec![0; candidate.len() + 1]; query.len() + 1];
    for (i, row) in distance.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in distance[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=query.len() {
        for j in 1..=candidate.len() {
            distance[i][j] = (distance[i - 1][j] + 1)
                .min(distance[i][j - 1] + 1)
                .min(distance[i - 1][j - 1] + usize::from(query[i - 1] != candidate[j - 1]));
            if i > 1
                && j > 1
                && query[i - 1] == candidate[j - 2]
                && query[i - 2] == candidate[j - 1]
            {
                distance[i][j] = distance[i][j].min(distance[i - 2][j - 2] + 1);
            }
        }
    }
    let result = distance[query.len()][candidate.len()];
    (result <= threshold).then_some(result)
}
