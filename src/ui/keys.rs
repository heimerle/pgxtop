//! One source of truth for the key bindings.
//!
//! The footer and the `?` overlay are both generated from here, and a test
//! asserts every advertised key is actually handled — the old static footer
//! advertised `Tab`, `Enter`, `s` and `?`, all of which were empty stubs.

use crate::app::View;
use crate::ui::state::Overlay;

pub type Binding = (&'static str, &'static str);

pub const GLOBAL: &[Binding] = &[
    ("1-6", "View"),
    ("r", "Refresh"),
    ("p", "Pause"),
    ("?", "Help"),
    ("q", "Quit"),
];

pub const MODELS: &[Binding] = &[
    ("↑↓/jk", "Select"),
    ("PgUp/PgDn", "Page"),
    ("g/G", "First/Last"),
    ("Enter", "Detail"),
    ("s", "Sort"),
    ("S", "Reverse"),
    ("t", "History"),
];

pub const MODELS_MULTI_ENGINE: &[Binding] = &[("Tab", "Filter")];

pub const OVERLAY_KEYS: &[Binding] = &[("Esc", "Close"), ("1-6", "View")];

/// The compact set shown in the footer.
pub fn footer(view: View, overlay: Overlay, multi_engine: bool) -> Vec<Binding> {
    if overlay != Overlay::None {
        return OVERLAY_KEYS.to_vec();
    }
    let mut out: Vec<Binding> = vec![("1-6", "View")];
    if view == View::Llm {
        out.extend_from_slice(&[("↑↓/jk", "Select"), ("Enter", "Detail"), ("s", "Sort")]);
        if multi_engine {
            out.extend_from_slice(MODELS_MULTI_ENGINE);
        }
    }
    out.extend_from_slice(&[("r", "Refresh"), ("p", "Pause"), ("?", "Help"), ("q", "Quit")]);
    out
}

/// The full set shown in the `?` overlay.
pub fn help(view: View) -> Vec<Binding> {
    let mut out: Vec<Binding> = GLOBAL.to_vec();
    if view == View::Llm {
        out.push(("", ""));
        out.extend_from_slice(MODELS);
        out.extend_from_slice(MODELS_MULTI_ENGINE);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every key the UI advertises must be handled by `App::handle_key`.
    #[test]
    fn every_advertised_key_is_handled() {
        let handled = crate::app::handled_keys();
        for view in [
            View::Overview,
            View::Gpu,
            View::Llm,
            View::Processes,
            View::System,
            View::Network,
        ] {
            for (k, _) in footer(view, Overlay::None, true)
                .into_iter()
                .chain(help(view))
            {
                if k.is_empty() {
                    continue;
                }
                assert!(handled.contains(&k), "{view:?} advertises unhandled key {k:?}");
            }
        }
        for (k, _) in footer(View::Llm, Overlay::Help, true) {
            assert!(handled.contains(&k), "overlay advertises unhandled key {k:?}");
        }
    }

    #[test]
    fn footer_only_offers_model_keys_on_the_models_view() {
        let llm = footer(View::Llm, Overlay::None, false);
        assert!(llm.iter().any(|(k, _)| *k == "Enter"));
        let system = footer(View::System, Overlay::None, false);
        assert!(!system.iter().any(|(k, _)| *k == "Enter"));
    }

    #[test]
    fn filter_is_hidden_with_a_single_engine() {
        assert!(!footer(View::Llm, Overlay::None, false)
            .iter()
            .any(|(k, _)| *k == "Tab"));
        assert!(footer(View::Llm, Overlay::None, true)
            .iter()
            .any(|(k, _)| *k == "Tab"));
    }

    #[test]
    fn an_open_overlay_advertises_only_what_still_works() {
        let f = footer(View::Llm, Overlay::Detail, true);
        assert!(f.iter().any(|(k, _)| *k == "Esc"));
        assert!(!f.iter().any(|(k, _)| *k == "Enter"));
        assert!(!f.iter().any(|(k, _)| *k == "s"));
    }
}
