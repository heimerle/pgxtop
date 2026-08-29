/// A bounded, **index-aligned** time series.
///
/// Every `push` appends to every series, using `f32::NAN` for a missing
/// sample. The previous history implementations pushed conditionally but
/// trimmed all series by an excess computed from one of them, which
/// desynchronised them and panicked in `drain(0..excess)` as soon as one
/// metric was always `None` — the normal case on a GPU without fan or
/// mem-clock support, and on any engine reporting only some metrics.
#[derive(Debug, Clone, Default)]
pub struct Series {
    values: Vec<f32>,
}

#[allow(dead_code)]
impl Series {
    pub fn with_capacity(n: usize) -> Self {
        Self { values: Vec::with_capacity(n) }
    }

    pub fn as_slice(&self) -> &[f32] {
        &self.values
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// True when every sample is NaN, i.e. the source never reported a value.
    pub fn is_all_missing(&self) -> bool {
        self.values.iter().all(|v| v.is_nan())
    }

    /// Largest finite sample, ignoring NaN gaps.
    pub fn max(&self) -> Option<f32> {
        self.values
            .iter()
            .copied()
            .filter(|v| v.is_finite())
            .fold(None, |acc: Option<f32>, v| Some(acc.map_or(v, |a| a.max(v))))
    }

    /// Most recent finite sample.
    pub fn last(&self) -> Option<f32> {
        self.values.iter().rev().copied().find(|v| v.is_finite())
    }

    pub fn push(&mut self, v: Option<f32>, max_points: usize) {
        self.values.push(v.unwrap_or(f32::NAN));
        if max_points > 0 && self.values.len() > max_points {
            let excess = self.values.len() - max_points;
            self.values.drain(0..excess);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_pads_missing_samples_and_stays_bounded() {
        let mut s = Series::with_capacity(3);
        for i in 0..10 {
            s.push(if i % 2 == 0 { Some(i as f32) } else { None }, 3);
        }
        assert_eq!(s.len(), 3);
        assert!(!s.is_all_missing());
    }

    #[test]
    fn all_missing_series_is_detected() {
        let mut s = Series::default();
        for _ in 0..5 {
            s.push(None, 10);
        }
        assert!(s.is_all_missing());
        assert_eq!(s.max(), None);
        assert_eq!(s.last(), None);
    }

    #[test]
    fn max_and_last_skip_nan_gaps() {
        let mut s = Series::default();
        s.push(Some(1.0), 10);
        s.push(None, 10);
        s.push(Some(7.0), 10);
        s.push(None, 10);
        assert_eq!(s.max(), Some(7.0));
        assert_eq!(s.last(), Some(7.0));
    }
}
