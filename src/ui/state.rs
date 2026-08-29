//! UI-side view model and selection state for the models screen.

use std::sync::Arc;
use std::time::Duration;

use ratatui::widgets::TableState;

use crate::collectors::inference::InferenceSnapshot;
use crate::models::inference::{EngineType, Expiry, ModelDetail, ModelStatus, ProcessorSplit};

/// One row of the models table, flattened from a [`ModelInstance`] plus the
/// engine context the renderer needs.
///
/// Built once per refresh rather than per frame, so the 10 Hz render loop does
/// no allocation or sorting.
/// `families` is parsed and carried for the detail overlay's future use.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ModelRow {
    /// Stable identity, used to keep the selection on the same *model* across
    /// rebuilds, sort changes and engine outages.
    pub key: String,
    pub name: String,
    pub engine_id: String,
    /// The engine's display name — the old table printed the raw `engine_id`.
    pub engine_label: String,
    pub engine_url: String,
    pub digest: Option<String>,
    pub family: Option<String>,
    pub families: Option<Vec<String>>,
    pub parameter_size: Option<String>,
    pub quantization: Option<String>,
    pub format: Option<String>,
    pub parent_model: Option<String>,
    pub capabilities: Vec<String>,
    pub size_total: Option<u64>,
    pub size_vram: Option<u64>,
    pub size_cpu: Option<u64>,
    pub processor: Option<ProcessorSplit>,
    pub context_size: Option<u32>,
    pub context_max: Option<u32>,
    pub expiry: Expiry,
    pub status: ModelStatus,
    pub detail: Option<Arc<ModelDetail>>,
    /// Whether this engine has a per-model detail endpoint at all. Only
    /// Ollama does (`/api/show`); for the rest the overlay must say so
    /// rather than spin on "loading" forever.
    pub supports_detail: bool,
    /// The owning engine has not answered recently; render dimmed.
    pub stale: bool,
}

impl ModelRow {
    pub fn gpu_fraction(&self) -> Option<f32> {
        self.processor.and_then(ProcessorSplit::gpu_fraction)
    }

    pub fn is_resident(&self) -> bool {
        self.status.is_resident()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ModelSort {
    #[default]
    Name,
    Engine,
    Size,
    Processor,
    Until,
}

impl ModelSort {
    pub fn next(self) -> Self {
        match self {
            Self::Name => Self::Engine,
            Self::Engine => Self::Size,
            Self::Size => Self::Processor,
            Self::Processor => Self::Until,
            Self::Until => Self::Name,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Name => "NAME",
            Self::Engine => "ENGINE",
            Self::Size => "SIZE",
            Self::Processor => "PROC",
            Self::Until => "UNTIL",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Overlay {
    #[default]
    None,
    Help,
    Detail,
}

#[derive(Debug, Default)]
pub struct ModelsUiState {
    pub table: TableState,
    /// The source of truth for the selection — not the table index.
    pub selected_key: Option<String>,
    pub sort: ModelSort,
    pub sort_desc: bool,
    /// `None` = all engines.
    pub filter_engine: Option<String>,
    /// Written by the renderer, read by PageUp/PageDown.
    pub visible_rows: usize,
    pub multi_engine: bool,
    pub any_engine_connected: bool,
    pub show_history: bool,
}

impl ModelsUiState {
    /// Re-anchors the selection after the row list has been rebuilt.
    ///
    /// Order matters: follow the same model first, then hold the same screen
    /// position, then fall back to the top. That is what makes the cursor stay
    /// put when a model loads or unloads underneath it.
    pub fn reconcile(&mut self, rows: &[ModelRow]) {
        if rows.is_empty() {
            self.table.select(None);
            // selected_key is deliberately kept: when the engine comes back,
            // the previous selection is restored.
            return;
        }
        let idx = self
            .selected_key
            .as_deref()
            .and_then(|k| rows.iter().position(|r| r.key == k))
            .or_else(|| self.table.selected().map(|i| i.min(rows.len() - 1)))
            .unwrap_or(0);
        self.select_index(idx, rows);
    }

    pub fn select_index(&mut self, idx: usize, rows: &[ModelRow]) {
        if rows.is_empty() {
            self.table.select(None);
            return;
        }
        let idx = idx.min(rows.len() - 1);
        self.table.select(Some(idx));
        self.selected_key = Some(rows[idx].key.clone());
    }

    /// Clamped, never wrapping — the same behaviour as btop and htop.
    pub fn move_by(&mut self, delta: isize, rows: &[ModelRow]) {
        if rows.is_empty() {
            return;
        }
        let cur = self.table.selected().unwrap_or(0) as isize;
        let next = (cur + delta).clamp(0, rows.len() as isize - 1) as usize;
        self.select_index(next, rows);
    }

    pub fn page(&self) -> isize {
        self.visible_rows.max(1) as isize
    }

    pub fn selected<'a>(&self, rows: &'a [ModelRow]) -> Option<&'a ModelRow> {
        self.table.selected().and_then(|i| rows.get(i))
    }
}

/// Flattens a snapshot into sorted, filtered rows.
pub fn build_rows(
    snapshot: &InferenceSnapshot,
    sort: ModelSort,
    desc: bool,
    filter_engine: Option<&str>,
) -> Vec<ModelRow> {
    let stale_after: Duration = snapshot.stale_after;

    let mut rows: Vec<ModelRow> = snapshot
        .engines
        .iter()
        .filter(|e| filter_engine.is_none_or(|f| e.engine.id == f))
        .flat_map(|e| {
            let stale = e.is_stale(stale_after);
            let label = e.engine.name.clone();
            let url = e.engine.url.clone();
            let supports_detail = e.engine.engine_type == EngineType::Ollama;
            e.models.iter().map(move |m| ModelRow {
                key: m.id.clone(),
                name: m.name.clone(),
                engine_id: m.engine_id.clone(),
                engine_label: label.clone(),
                engine_url: url.clone(),
                digest: m.digest.clone(),
                family: m.family.clone(),
                families: m.families.clone(),
                parameter_size: m.parameter_size.clone(),
                quantization: m.quantization.clone(),
                format: m.format.clone(),
                parent_model: m.parent_model.clone(),
                capabilities: m.capabilities.clone(),
                size_total: m.size_total,
                size_vram: m.size_vram,
                size_cpu: m.size_cpu,
                processor: m.processor,
                context_size: m.context_size,
                context_max: m.context_max,
                expiry: m.expiry,
                status: m.status,
                detail: m.detail.clone(),
                supports_detail,
                stale,
            })
        })
        .collect();

    sort_rows(&mut rows, sort, desc);
    rows
}

/// Sorting happens here, never in the renderer, and always ends in a total
/// order — otherwise rows would reorder on VRAM jitter and the cursor would
/// appear to jump.
pub fn sort_rows(rows: &mut [ModelRow], sort: ModelSort, desc: bool) {
    rows.sort_by(|a, b| {
        // Resident first, then merely-offered, then the on-disk catalogue.
        let residency = a.status.rank().cmp(&b.status.rank());
        if residency != std::cmp::Ordering::Equal {
            return residency;
        }

        let primary = match sort {
            ModelSort::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            ModelSort::Engine => a
                .engine_label
                .to_lowercase()
                .cmp(&b.engine_label.to_lowercase()),
            // Biggest first is the useful default for a size sort.
            ModelSort::Size => b.size_total.cmp(&a.size_total),
            ModelSort::Processor => order_f32(a.gpu_fraction(), b.gpu_fraction()),
            ModelSort::Until => order_expiry(&a.expiry, &b.expiry),
        };
        let primary = if desc { primary.reverse() } else { primary };
        primary.then_with(|| a.key.cmp(&b.key))
    });
}

fn order_f32(a: Option<f32>, b: Option<f32>) -> std::cmp::Ordering {
    match (a, b) {
        // Most-offloaded first: that is what the user needs to act on.
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

fn order_expiry(a: &Expiry, b: &Expiry) -> std::cmp::Ordering {
    fn rank(e: &Expiry) -> u8 {
        match e {
            Expiry::At(_) => 0,
            Expiry::Forever => 1,
            Expiry::Never => 2,
            Expiry::Unknown => 3,
        }
    }
    rank(a).cmp(&rank(b)).then_with(|| match (a, b) {
        (Expiry::At(x), Expiry::At(y)) => x.cmp(y),
        _ => std::cmp::Ordering::Equal,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str) -> ModelRow {
        ModelRow {
            key: format!("ollama/{name}"),
            name: name.into(),
            engine_id: "ollama".into(),
            engine_label: "Ollama".into(),
            engine_url: "http://localhost:11434".into(),
            digest: None,
            family: None,
            families: None,
            parameter_size: None,
            quantization: None,
            format: None,
            parent_model: None,
            capabilities: Vec::new(),
            size_total: None,
            size_vram: None,
            size_cpu: None,
            processor: None,
            context_size: None,
            context_max: None,
            expiry: Expiry::Unknown,
            status: ModelStatus::Loaded,
            detail: None,
            supports_detail: true,
            stale: false,
        }
    }

    fn rows(names: &[&str]) -> Vec<ModelRow> {
        names.iter().map(|n| row(n)).collect()
    }

    /// The property that matters most: at 500 ms the list is rebuilt
    /// constantly, and an index-based selection would wander.
    #[test]
    fn selection_follows_the_model_across_rebuilds() {
        let mut ui = ModelsUiState::default();

        let a = rows(&["llama", "qwen", "glm"]);
        ui.reconcile(&a);
        ui.move_by(1, &a);
        assert_eq!(ui.selected_key.as_deref(), Some("ollama/qwen"));

        // Next tick: llama unloaded, phi loaded, order changed.
        let b = rows(&["glm", "phi", "qwen"]);
        ui.reconcile(&b);
        assert_eq!(ui.table.selected(), Some(2), "index moved");
        assert_eq!(ui.selected_key.as_deref(), Some("ollama/qwen"), "same model");
    }

    #[test]
    fn selection_clamps_when_the_list_shrinks() {
        let mut ui = ModelsUiState::default();
        let a = rows(&["a", "b", "c"]);
        ui.reconcile(&a);
        ui.select_index(2, &a);

        let b = rows(&["a", "b"]);
        ui.reconcile(&b);
        assert_eq!(ui.table.selected(), Some(1));
        assert_eq!(ui.selected_key.as_deref(), Some("ollama/b"));
    }

    #[test]
    fn empty_list_clears_the_cursor_but_remembers_the_model() {
        let mut ui = ModelsUiState::default();
        let a = rows(&["a"]);
        ui.reconcile(&a);
        assert_eq!(ui.selected_key.as_deref(), Some("ollama/a"));

        ui.reconcile(&[]);
        assert_eq!(ui.table.selected(), None);
        // Kept, so a reconnecting engine restores the selection.
        assert_eq!(ui.selected_key.as_deref(), Some("ollama/a"));

        ui.reconcile(&a);
        assert_eq!(ui.table.selected(), Some(0));
    }

    #[test]
    fn movement_clamps_and_never_wraps() {
        let mut ui = ModelsUiState::default();
        let r = rows(&["a", "b", "c"]);
        ui.reconcile(&r);
        ui.move_by(-5, &r);
        assert_eq!(ui.table.selected(), Some(0));
        ui.move_by(99, &r);
        assert_eq!(ui.table.selected(), Some(2));
        ui.move_by(0, &[]);
        assert_eq!(ui.table.selected(), Some(2));
    }

    /// Resident first, then merely-offered router routes, then the on-disk
    /// catalogue — regardless of the sort column.
    #[test]
    fn residency_outranks_the_sort_column() {
        let mut r = rows(&["zzz-loaded", "aaa-installed", "mmm-served"]);
        r[1].status = ModelStatus::Installed;
        r[2].status = ModelStatus::Served;

        sort_rows(&mut r, ModelSort::Name, false);
        let names: Vec<&str> = r.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, vec!["zzz-loaded", "mmm-served", "aaa-installed"]);

        // Reversing the sort direction must not promote a route above a
        // genuinely resident model.
        sort_rows(&mut r, ModelSort::Name, true);
        assert_eq!(r[0].name, "zzz-loaded");
        assert_eq!(r[2].name, "aaa-installed");
    }

    #[test]
    fn sort_is_a_total_order_so_rows_cannot_jitter() {
        let mut r = rows(&["b", "a", "c"]);
        for row in r.iter_mut() {
            row.size_total = Some(100); // all equal: the tiebreak must decide
        }
        sort_rows(&mut r, ModelSort::Size, false);
        let first: Vec<String> = r.iter().map(|x| x.name.clone()).collect();
        sort_rows(&mut r, ModelSort::Size, false);
        let again: Vec<String> = r.iter().map(|x| x.name.clone()).collect();
        assert_eq!(first, again);
        assert_eq!(first, vec!["a", "b", "c"]);
    }

    #[test]
    fn processor_sort_puts_the_most_offloaded_first() {
        let mut r = rows(&["full", "half", "unknown"]);
        r[0].processor = Some(ProcessorSplit::AllGpu);
        r[1].processor = Some(ProcessorSplit::Split { cpu_pct: 50, gpu_pct: 50 });
        r[2].processor = None;
        sort_rows(&mut r, ModelSort::Processor, false);
        assert_eq!(r[0].name, "half");
        assert_eq!(r[1].name, "full");
        assert_eq!(r[2].name, "unknown");
    }

    #[test]
    fn sort_cycles_through_every_column() {
        let mut s = ModelSort::Name;
        let mut seen = vec![s];
        for _ in 0..4 {
            s = s.next();
            seen.push(s);
        }
        assert_eq!(seen.len(), 5);
        assert_eq!(s.next(), ModelSort::Name);
    }
}
