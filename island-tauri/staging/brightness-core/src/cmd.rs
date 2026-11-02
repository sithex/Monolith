//! Command builders: `Step` → `StepRequest` for PowerDevil's
//! `AdjustBrightnessStepWithContext`. Nothing here ever calls D-Bus.

use crate::{CONTEXT, SUPPRESS_INDICATOR};
use serde::{Deserialize, Serialize};

/// `AdjustBrightnessStepWithContext` step actions (verbatim).
pub const STEP_INCREASE: u32 = 0;
pub const STEP_DECREASE: u32 = 1;
pub const STEP_INCREASE_SMALL: u32 = 2;
pub const STEP_DECREASE_SMALL: u32 = 3;

/// A single brightness step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Step {
    Increase,
    Decrease,
    IncreaseSmall,
    DecreaseSmall,
}

impl Step {
    fn action(self) -> u32 {
        match self {
            Step::Increase => STEP_INCREASE,
            Step::Decrease => STEP_DECREASE,
            Step::IncreaseSmall => STEP_INCREASE_SMALL,
            Step::DecreaseSmall => STEP_DECREASE_SMALL,
        }
    }
}

/// The `AdjustBrightnessStepWithContext` method-call arguments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepRequest {
    pub step_action: u32,
    pub flags: u32,
    pub context: String,
}

/// Build a step request. `flags` is `SUPPRESS_INDICATOR`; `context` is
/// `CONTEXT`, passed through unchanged into BrightnessChanged.
pub fn step_request(step: Step) -> StepRequest {
    StepRequest {
        step_action: step.action(),
        flags: SUPPRESS_INDICATOR,
        context: CONTEXT.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_mapping() {
        let cases = [
            (Step::Increase, STEP_INCREASE),
            (Step::Decrease, STEP_DECREASE),
            (Step::IncreaseSmall, STEP_INCREASE_SMALL),
            (Step::DecreaseSmall, STEP_DECREASE_SMALL),
        ];
        for (step, action) in cases {
            let r = step_request(step);
            assert_eq!(r.step_action, action);
            assert_eq!(r.flags, SUPPRESS_INDICATOR);
            assert_eq!(r.context, CONTEXT);
        }
    }
}
