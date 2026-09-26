use crate::model::macros::*;
use std::collections::BTreeSet;
fn valid_choices(choices: &[Choice]) -> bool {
    let ids: BTreeSet<_> = choices.iter().map(|choice| choice.id.as_str()).collect();
    ids.len() == choices.len() && !ids.contains("")
}

pub fn validate_capabilities(capabilities: &Capabilities) -> Result<(), String> {
    if capabilities.backend_id.is_empty()
        || capabilities.slots.is_empty()
        || !valid_choices(&capabilities.slots)
        || !valid_choices(&capabilities.backend_actions)
        || capabilities.repeat_counts.is_empty()
        || capabilities.editable_repeat_counts.is_empty()
        || !capabilities
            .repeat_counts
            .contains(capabilities.editable_repeat_counts.start())
        || !capabilities
            .repeat_counts
            .contains(capabilities.editable_repeat_counts.end())
        || capabilities.delays_ms.is_empty()
        || capabilities
            .keys
            .as_ref()
            .is_some_and(RangeInclusive::is_empty)
        || capabilities
            .movement
            .as_ref()
            .is_some_and(RangeInclusive::is_empty)
    {
        return Err("Invalid macro capability IDs or ranges".into());
    }
    let buttons: BTreeSet<_> = capabilities
        .buttons
        .iter()
        .map(|choice| choice.button)
        .collect();
    if buttons.len() != capabilities.buttons.len() || buttons.contains(&0) {
        return Err("Invalid macro pointer button capabilities".into());
    }
    if let Some(budget) = &capabilities.byte_budget
        && (budget.overhead > budget.limit
            || budget.inline_delays.is_empty()
            || [
                (capabilities.keys.is_some(), budget.key),
                (!capabilities.buttons.is_empty(), budget.button),
                (capabilities.movement.is_some(), budget.movement),
                (!capabilities.backend_actions.is_empty(), budget.backend),
            ]
            .into_iter()
            .any(|(supported, cost)| supported && cost == 0))
    {
        return Err("Invalid macro byte budget".into());
    }
    let slots: BTreeSet<_> = capabilities
        .slots
        .iter()
        .map(|slot| slot.id.as_str())
        .collect();
    let mut bindings = BTreeSet::new();
    for binding in &capabilities.bindings {
        if !slots.contains(binding.slot.as_str())
            || matches!(binding.action, crate::model::keymap::Action::Opaque { .. })
            || binding.id.is_empty()
            || binding.label.is_empty()
            || binding
                .required_repeat_count
                .is_some_and(|count| !capabilities.editable_repeat_counts.contains(&count))
            || !bindings.insert((binding.slot.as_str(), binding.id.as_str()))
        {
            return Err("Invalid macro binding capabilities".into());
        }
    }
    Ok(())
}

pub fn validate_program(capabilities: &Capabilities, program: &Program) -> Result<(), String> {
    validate_capabilities(capabilities)?;
    if !capabilities.repeat_counts.contains(&program.repeat_count) {
        return Err("Macro repeat count is outside backend limits".into());
    }
    let mut encoded_size = capabilities
        .byte_budget
        .as_ref()
        .map(|budget| budget.overhead);
    for (index, event) in program.events.iter().enumerate() {
        if !capabilities.delays_ms.contains(&event.delay_ms) {
            return Err(format!("Event {index}: wait is outside backend limits"));
        }
        let supported = match &event.action {
            Action::Key { usage, .. } => capabilities
                .keys
                .as_ref()
                .is_some_and(|range| range.contains(usage)),
            Action::Button { button, .. } => capabilities
                .buttons
                .iter()
                .any(|choice| choice.button == *button),
            Action::Move { dx, dy } => capabilities
                .movement
                .as_ref()
                .is_some_and(|range| range.contains(dx) && range.contains(dy)),
            Action::Backend { backend_id, id, .. } => {
                backend_id == &capabilities.backend_id
                    && capabilities
                        .backend_actions
                        .iter()
                        .any(|choice| choice.id == *id)
            }
        };
        if !supported {
            return Err(format!(
                "Event {index}: action is unsupported by this backend"
            ));
        }
        if let (Some(budget), Some(size)) = (&capabilities.byte_budget, &mut encoded_size) {
            let action_bytes = match &event.action {
                Action::Key { .. } => budget.key,
                Action::Button { .. } => budget.button,
                Action::Move { .. } => budget.movement,
                Action::Backend { .. } => budget.backend,
            };
            *size = size
                .checked_add(action_bytes)
                .and_then(|size| {
                    size.checked_add(if budget.inline_delays.contains(&event.delay_ms) {
                        0
                    } else {
                        budget.extended_delay
                    })
                })
                .ok_or_else(|| "Macro encoded size overflows byte budget".to_owned())?;
            if *size > budget.limit {
                return Err(format!("Event {index}: macro exceeds byte budget"));
            }
        }
    }
    Ok(())
}

use std::ops::RangeInclusive;
