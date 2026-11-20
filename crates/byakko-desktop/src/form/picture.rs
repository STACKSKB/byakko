//! Per-key paint and local look previews. Only Apply stages a look in core.
use crate::widget::color_picker::{Gesture, Interaction};
use byakko_core::{
    editor::{
        Editor, Status,
        picture::{Colors, PictureRules},
    },
    model::picture::{Content, Edit, Snapshot},
};
use byakko_devices::nia87::looks::Look;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Mode {
    #[default]
    Inspect,
    Paint,
}

#[derive(Clone, Debug)]
pub enum Message {
    Select(String),
    Paint(String),
    Mode(Mode),
    Brush([u8; 3]),
    Picker(Interaction),
    Preview(Look),
    ReplacePainted(bool),
    ApplyPreview,
    RevertPreview,
    Read,
    Revert,
}

struct Preview {
    look: Look,
    brush: [u8; 3],
    before: Colors,
    pin: Snapshot,
    corrections: BTreeMap<String, [u8; 3]>,
    replace_painted: bool,
    applying: bool,
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd)]
struct PictureContext {
    generation: u64,
    backend_id: String,
    selector: Vec<u8>,
}

#[derive(Default)]
pub struct Form {
    pub(crate) selected: Option<String>,
    brush: Option<[u8; 3]>,
    gesture: Gesture,
    mode: Mode,
    painted: BTreeSet<String>,
    painted_by_context: BTreeMap<PictureContext, BTreeSet<String>>,
    active_context: Option<PictureContext>,
    preview: Option<Preview>,
}

impl Form {
    /// Manual paint ownership belongs to a connected board and picture selector.
    /// Unknown colors read from a device are never classified as hand-painted.
    pub fn observed_picture(&mut self, editor: &Editor<PictureRules>, generation: u64) {
        let Some(snapshot) = editor.baseline() else {
            return;
        };
        let next = PictureContext {
            generation,
            backend_id: snapshot.backend_id.clone(),
            selector: snapshot.context_revision.clone(),
        };
        if self.active_context.as_ref() == Some(&next) {
            return;
        }
        if self
            .active_context
            .as_ref()
            .is_some_and(|old| old.generation != generation)
        {
            self.painted_by_context.clear();
        } else if let Some(old) = self.active_context.take() {
            self.painted_by_context.insert(old, self.painted.clone());
        }
        self.painted = self
            .painted_by_context
            .get(&next)
            .cloned()
            .unwrap_or_default();
        self.active_context = Some(next);
        self.brush = None;
        self.gesture = Gesture::Idle;
        self.preview = None;
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }
    pub fn painted_count(&self) -> usize {
        self.painted.len()
    }
    pub fn preview_look(&self) -> Option<Look> {
        self.preview.as_ref().map(|p| p.look)
    }
    pub fn replacing_painted(&self) -> bool {
        self.preview.as_ref().is_some_and(|p| p.replace_painted)
    }
    pub fn applying_preview(&self) -> bool {
        self.preview.as_ref().is_some_and(|p| p.applying)
    }
    pub fn dragging(&self) -> bool {
        self.gesture == Gesture::Dragging
    }

    pub fn color(&self, editor: &Editor<PictureRules>) -> Option<[u8; 3]> {
        self.brush.or_else(|| {
            let selected = self.selected.as_ref()?;
            self.display_colors(editor).get(selected).copied()
        })
    }

    pub fn selected_colors(&self, editor: &Editor<PictureRules>) -> Option<([u8; 3], [u8; 3])> {
        let selected = self.selected.as_ref()?;
        let device = editor.baseline().and_then(|s| match &s.content {
            Content::Editable(colors) => colors.get(selected).copied(),
            Content::Opaque { .. } => None,
        })?;
        let displayed = self.display_colors(editor).get(selected).copied()?;
        Some((device, displayed))
    }

    pub fn display_colors(&self, editor: &Editor<PictureRules>) -> Colors {
        self.preview_colors(editor)
            .unwrap_or_else(|| editor.draft().cloned().unwrap_or_default())
    }

    fn exact_capability_map(editor: &Editor<PictureRules>, colors: &Colors) -> bool {
        editor.capabilities().backend_id == "nia87"
            && colors.keys().cloned().collect::<BTreeSet<_>>()
                == editor.capabilities().keys.iter().cloned().collect()
    }

    fn pin_matches(preview: &Preview, editor: &Editor<PictureRules>) -> bool {
        editor.status() == &Status::Ready
            && editor.baseline().is_some_and(|baseline| {
                baseline.backend_id == preview.pin.backend_id
                    && baseline.revision == preview.pin.revision
                    && baseline.context_revision == preview.pin.context_revision
            })
    }

    pub fn preview_colors(&self, editor: &Editor<PictureRules>) -> Option<Colors> {
        let preview = self.preview.as_ref()?;
        if !Self::pin_matches(preview, editor) {
            return None;
        }
        let mut colors = preview.look.colors(preview.brush);
        if !Self::exact_capability_map(editor, &colors) {
            return None;
        }
        if !preview.replace_painted {
            for key in &self.painted {
                if let Some(color) = editor.draft().and_then(|draft| draft.get(key)) {
                    colors.insert(key.clone(), *color);
                }
            }
        }
        colors.extend(
            preview
                .corrections
                .iter()
                .map(|(key, color)| (key.clone(), *color)),
        );
        Some(colors)
    }

    pub fn start_preview(
        &mut self,
        look: Look,
        editor: &Editor<PictureRules>,
    ) -> Result<(), String> {
        if editor.status() != &Status::Ready || editor.submitted().is_some() {
            return Err("Wait for colors to be ready".into());
        }
        let colors = look.colors(self.brush.unwrap_or([255, 32, 96]));
        if !Self::exact_capability_map(editor, &colors) {
            return Err("This keyboard does not support this look".into());
        }
        let baseline = editor.baseline().ok_or("Read colors first")?;
        let before = editor.draft().ok_or("Colors are not editable")?;
        if let Some(preview) = &mut self.preview {
            if !Self::pin_matches(preview, editor) {
                return Err("Read colors before changing the preview".into());
            }
            if preview.applying {
                return Err("Wait for the look to finish applying".into());
            }
            preview.look = look;
            preview.brush = self.brush.unwrap_or([255, 32, 96]);
        } else {
            self.preview = Some(Preview {
                look,
                brush: self.brush.unwrap_or([255, 32, 96]),
                before: before.clone(),
                pin: baseline.clone(),
                corrections: BTreeMap::new(),
                replace_painted: false,
                applying: false,
            });
        }
        Ok(())
    }

    pub fn set_replace_painted(&mut self, replace: bool) {
        if let Some(preview) = &mut self.preview
            && !preview.applying
        {
            preview.replace_painted = replace;
        }
    }

    /// Revert is preview-only: ordinary pending/accepted paints remain in core.
    pub fn revert_preview(&mut self) {
        if self
            .preview
            .as_ref()
            .is_some_and(|preview| !preview.applying)
        {
            self.preview = None;
        }
    }

    pub fn target(&self, editor: &Editor<PictureRules>) -> Result<Snapshot, String> {
        let preview = self.preview.as_ref().ok_or("Choose a look first")?;
        if preview.applying {
            return Err("A look is already applying".into());
        }
        if editor.submitted().is_some() {
            return Err("Wait for the current color write".into());
        }
        let colors = self
            .preview_colors(editor)
            .ok_or("Read colors before applying this look")?;
        let mut target = preview.pin.clone();
        target.content = Content::Editable(colors);
        Ok(target)
    }

    pub fn before_target(&self) -> Option<Snapshot> {
        let preview = self.preview.as_ref()?;
        let mut target = preview.pin.clone();
        target.content = Content::Editable(preview.before.clone());
        Some(target)
    }

    pub fn mark_applying(&mut self) {
        if let Some(preview) = &mut self.preview {
            preview.applying = true;
        }
    }

    pub fn apply_failed(&mut self) {
        if let Some(preview) = &mut self.preview {
            preview.applying = false;
        }
    }

    pub fn apply_succeeded(&mut self) {
        if let Some(preview) = self.preview.take() {
            if preview.replace_painted {
                self.painted.clear();
            }
            self.painted.extend(preview.corrections.keys().cloned());
        }
    }

    /// A pending ordinary paint may finish while the local preview is open.
    /// Re-pin only if its accepted colors equal the pre-preview draft.
    pub fn ordinary_paint_saved(&mut self, editor: &Editor<PictureRules>) {
        let Some(preview) = &mut self.preview else {
            return;
        };
        let Some(baseline) = editor.baseline() else {
            return;
        };
        if baseline.backend_id == preview.pin.backend_id
            && baseline.context_revision == preview.pin.context_revision
            && editor.draft() == Some(&preview.before)
            && matches!(&baseline.content, Content::Editable(colors) if colors == &preview.before)
        {
            preview.pin = baseline.clone();
        }
    }

    /// Brush choice follows accepted paint edits, never a rejected intent.
    pub fn accepted(&mut self, key: &str, editor: &Editor<PictureRules>) {
        if self.preview.is_some() {
            if let Some(color) = editor.draft().and_then(|colors| colors.get(key))
                && let Some(preview) = &mut self.preview
            {
                preview.corrections.insert(key.to_owned(), *color);
            }
        } else if let Some(color) = editor.draft().and_then(|colors| colors.get(key)) {
            self.brush = Some(*color);
            self.painted.insert(key.to_owned());
        }
    }

    pub fn update(&mut self, message: Message, editor: &Editor<PictureRules>) -> Option<Edit> {
        match message {
            Message::Select(key) => {
                if !editor.capabilities().keys.contains(&key) {
                    return None;
                }
                self.selected = Some(key);
                self.gesture = Gesture::Idle;
            }
            Message::Paint(key) => {
                if self.mode != Mode::Paint || !editor.capabilities().keys.contains(&key) {
                    return None;
                }
                self.selected = Some(key.clone());
                let color = self.brush.or_else(|| editor.draft()?.get(&key).copied())?;
                if let Some(preview) = &mut self.preview {
                    if !preview.applying {
                        preview.corrections.insert(key, color);
                    }
                    return None;
                }
                return Some(Edit::Color { key, color });
            }
            Message::Mode(mode) => self.mode = mode,
            Message::Brush(color) => self.brush = Some(color),
            Message::Picker(Interaction::Started) => self.gesture = Gesture::Dragging,
            Message::Picker(Interaction::Finished) => self.gesture = Gesture::Idle,
            Message::Picker(Interaction::Moved)
            | Message::Preview(_)
            | Message::ReplacePainted(_)
            | Message::ApplyPreview
            | Message::RevertPreview
            | Message::Read
            | Message::Revert => {}
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byakko_core::model::picture::{Capabilities, Evidence};

    fn editor() -> Editor<PictureRules> {
        let keys = Look::Wasd.colors([1, 2, 3]).into_keys().collect();
        let mut editor = Editor::new(PictureRules::new(Capabilities {
            backend_id: "nia87".into(),
            keys,
            lighting_effect: None,
        }));
        editor.accept_read(Ok(Snapshot {
            backend_id: "nia87".into(),
            revision: vec![1],
            context_revision: vec![7],
            evidence: Evidence::Readback,
            content: Content::Editable(
                editor
                    .capabilities()
                    .keys
                    .iter()
                    .map(|key| (key.clone(), [1, 2, 3]))
                    .collect(),
            ),
        }));
        editor
    }

    #[test]
    fn inspect_is_read_only_and_preview_does_not_stage_or_submit() {
        let editor = editor();
        let key = editor.capabilities().keys[0].clone();
        let before = editor.draft().unwrap().clone();
        let mut form = Form::default();
        assert!(form.update(Message::Select(key.clone()), &editor).is_none());
        assert_eq!(editor.draft(), Some(&before));
        form.start_preview(Look::Rainbow, &editor).unwrap();
        assert_eq!(editor.draft(), Some(&before));
        assert!(editor.submitted().is_none());
        assert_ne!(form.display_colors(&editor), before);
        form.revert_preview();
        assert_eq!(form.display_colors(&editor), before);
        assert!(!editor.dirty());
    }

    #[test]
    fn painted_keys_survive_look_switch_and_preview_corrections_until_apply() {
        let mut editor = editor();
        let keys = editor.capabilities().keys.clone();
        let (painted, corrected) = (&keys[0], &keys[1]);
        let mut form = Form::default();
        form.update(Message::Mode(Mode::Paint), &editor);
        form.update(Message::Brush([9, 8, 7]), &editor);
        let edit = form
            .update(Message::Paint(painted.clone()), &editor)
            .unwrap();
        editor.edit(edit).unwrap();
        form.accepted(painted, &editor);
        form.start_preview(Look::Wasd, &editor).unwrap();
        assert_eq!(form.display_colors(&editor)[painted], [9, 8, 7]);
        form.update(Message::Brush([6, 5, 4]), &editor);
        assert!(
            form.update(Message::Paint(corrected.clone()), &editor)
                .is_none()
        );
        form.start_preview(Look::Rainbow, &editor).unwrap();
        assert_eq!(form.display_colors(&editor)[corrected], [6, 5, 4]);
        assert_eq!(form.display_colors(&editor)[painted], [9, 8, 7]);
        let target = form.target(&editor).unwrap();
        assert_eq!(target.revision, vec![1]);
        assert_eq!(target.context_revision, vec![7]);
        let Content::Editable(target_colors) = target.content else {
            panic!()
        };
        assert_eq!(target_colors[corrected], [6, 5, 4]);
        form.set_replace_painted(true);
        assert_ne!(form.display_colors(&editor)[painted], [9, 8, 7]);
        form.revert_preview();
        assert_eq!(editor.draft().unwrap()[painted], [9, 8, 7]);
        assert_eq!(editor.draft().unwrap()[corrected], [1, 2, 3]);
    }

    #[test]
    fn selector_change_invalidates_local_preview_target() {
        let mut editor = editor();
        let mut form = Form::default();
        form.start_preview(Look::Rainbow, &editor).unwrap();
        let mut changed = editor.baseline().unwrap().clone();
        changed.context_revision = vec![8];
        editor.accept_read(Ok(changed));
        assert!(form.target(&editor).is_err());
        assert_eq!(
            form.display_colors(&editor),
            editor.draft().unwrap().clone()
        );
    }

    #[test]
    fn accepted_ordinary_paint_rebases_local_preview_without_sending_it() {
        let mut editor = editor();
        let key = editor.capabilities().keys[0].clone();
        let mut form = Form::default();
        form.update(Message::Mode(Mode::Paint), &editor);
        form.update(Message::Brush([9, 8, 7]), &editor);
        let paint = form.update(Message::Paint(key.clone()), &editor).unwrap();
        editor.edit(paint).unwrap();
        form.accepted(&key, &editor);
        form.start_preview(Look::Rainbow, &editor).unwrap();
        let expected_paint = editor.draft().unwrap().clone();
        let (_, submitted) = editor.request_apply().unwrap();
        assert_eq!(submitted, expected_paint);
        let mut accepted = editor.baseline().unwrap().clone();
        accepted.revision = vec![2];
        accepted.evidence = Evidence::TransportAccepted;
        accepted.content = Content::Editable(submitted);
        editor.accept_apply(Ok(accepted));
        assert!(form.target(&editor).is_err());
        form.ordinary_paint_saved(&editor);
        assert!(form.target(&editor).is_ok());
        assert_eq!(form.display_colors(&editor)[&key], [9, 8, 7]);
    }

    #[test]
    fn paint_ownership_follows_connection_and_selector_not_reread_revision() {
        let mut editor = editor();
        let key = editor.capabilities().keys[0].clone();
        let mut form = Form::default();
        form.observed_picture(&editor, 11);
        assert_eq!(
            form.painted_count(),
            0,
            "unknown device colors have no manual provenance"
        );
        form.update(Message::Mode(Mode::Paint), &editor);
        form.update(Message::Brush([9, 8, 7]), &editor);
        let paint = form.update(Message::Paint(key), &editor).unwrap();
        editor.edit(paint).unwrap();
        form.accepted(&editor.capabilities().keys[0], &editor);
        let (_, submitted) = editor.request_apply().unwrap();
        let mut saved = editor.baseline().unwrap().clone();
        saved.revision = vec![2];
        saved.evidence = Evidence::TransportAccepted;
        saved.content = Content::Editable(submitted);
        editor.accept_apply(Ok(saved.clone()));
        form.observed_picture(&editor, 11);
        assert_eq!(form.painted_count(), 1);
        editor.accept_read(Ok(Snapshot {
            evidence: Evidence::Readback,
            ..saved
        }));
        form.observed_picture(&editor, 11);
        assert_eq!(
            form.painted_count(),
            1,
            "same-selector reread retains ownership"
        );
        let mut layer_b = editor.baseline().unwrap().clone();
        layer_b.revision = vec![3];
        layer_b.context_revision = vec![8];
        editor.accept_read(Ok(layer_b));
        form.observed_picture(&editor, 11);
        assert_eq!(form.painted_count(), 0);
        let mut layer_a = editor.baseline().unwrap().clone();
        layer_a.revision = vec![4];
        layer_a.context_revision = vec![7];
        editor.accept_read(Ok(layer_a));
        form.observed_picture(&editor, 11);
        assert_eq!(
            form.painted_count(),
            1,
            "return to the same layer restores ownership"
        );
        form.observed_picture(&editor, 12);
        assert_eq!(
            form.painted_count(),
            0,
            "a new connection has unknown provenance"
        );
    }
}
