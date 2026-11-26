//! What a layer is before anything runs: switched off, not built, or to be run.

use crate::report::{Layer, LayerId, Verdict};

/// Layers this version builds: gates 1 through 4. Everything else is not built yet.
pub fn is_built(id: LayerId) -> bool {
    matches!(
        id,
        LayerId::Source | LayerId::Lint | LayerId::Eval | LayerId::BuildTest
    )
}

/// Gates 1 and 3 cannot be switched off; the others can, with a reason.
pub fn can_switch_off(id: LayerId) -> bool {
    !matches!(id, LayerId::Source | LayerId::Eval)
}

/// The layer as it stands without running it, or `None` when it has to run.
///
/// - switched off with a reason: `skip`, showing that reason;
/// - on and not built here: `not-run` (which fails the gate);
/// - otherwise it runs.
pub fn fixed_state(id: LayerId, switched_off: Option<&str>) -> Option<Layer> {
    if let Some(reason) = switched_off.filter(|_| can_switch_off(id)) {
        return Some(Layer::new(id, Verdict::Skip, Some(reason.to_string())));
    }
    if !is_built(id) {
        return Some(Layer::new(
            id,
            Verdict::NotRun,
            Some("not built in this version".to_string()),
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::ALL_LAYERS;

    #[test]
    fn gates_one_through_four_are_built_and_one_and_three_cannot_switch_off() {
        let built: Vec<LayerId> = ALL_LAYERS.into_iter().filter(|id| is_built(*id)).collect();
        assert_eq!(
            built,
            [
                LayerId::Source,
                LayerId::Lint,
                LayerId::Eval,
                LayerId::BuildTest
            ]
        );
        assert!(!can_switch_off(LayerId::Source) && !can_switch_off(LayerId::Eval));
        assert!(can_switch_off(LayerId::Lint));
        assert!(can_switch_off(LayerId::BuildTest));
        assert!(can_switch_off(LayerId::Laws));
    }

    #[test]
    fn a_switched_off_layer_is_skip_with_its_reason() {
        let layer = fixed_state(LayerId::Lint, Some("not built in M1")).unwrap();
        assert_eq!(
            (layer.verdict, layer.reason.as_deref()),
            (Verdict::Skip, Some("not built in M1"))
        );
    }

    #[test]
    fn an_unbuilt_layer_that_is_on_is_not_run_and_fails() {
        let layer = fixed_state(LayerId::Coverage, None).unwrap();
        assert_eq!(layer.verdict, Verdict::NotRun);
        assert!(layer.verdict.fails());
        assert_eq!(layer.reason.as_deref(), Some("not built in this version"));
    }

    #[test]
    fn a_built_layer_runs_and_cannot_be_switched_off() {
        assert_eq!(fixed_state(LayerId::Source, None), None);
        assert_eq!(fixed_state(LayerId::Lint, None), None);
        assert_eq!(fixed_state(LayerId::BuildTest, None), None);
        assert_eq!(fixed_state(LayerId::Eval, Some("because")), None);
    }
}
