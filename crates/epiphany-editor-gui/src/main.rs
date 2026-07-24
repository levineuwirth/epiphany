#![forbid(unsafe_code)]
//! # epiphany-editor-gui
//!
//! A **thin native GUI shell** over [`epiphany_editor_core::EditorSession`]: the
//! smallest real surface that proves the editor seam end to end. It renders the
//! session's resolved layout with `epiphany-render-svg`, rasterizes that SVG with
//! `resvg` into an `egui` texture, resolves clicks back to world coordinates to
//! select, and drives the note-editing intents from a toolbar and keyboard.
//!
//! A thin but real editing surface over the headless session: click to select, a
//! toolbar and keys for the note-editing intents, **undo/redo**
//! ([`EditorSession::undo`] / [`EditorSession::redo`]; Ctrl/Cmd+Z undo,
//! Ctrl/Cmd+Shift+Z or Ctrl/Cmd+Y redo), a
//! **pencil mode** that turns a click into a click-to-insert
//! ([`EditorSession::insert_note_at`]) with make-room overwrite, snapping to the
//! meter's beat grid ([`EditorSession::default_grid_at`]),
//! and a **duration palette** ([`EditorSession::set_selection_duration`]). The debug
//! panel shows the selection and the last applied op. The GUI is the thing meant to
//! surface the next real core gaps.
//!
//! It is a demo binary; there is no headless way to assert its rendering here, so the
//! one piece of nontrivial logic — the screen↔world coordinate map a click depends on
//! — is a pure [`ViewMap`] with a round-trip unit test.

use eframe::egui;

use epiphany_core::NoteValue;
use epiphany_editor_core::{EditOutcome, EditorError, EditorSession, GridResolution};
use epiphany_engrave::Engraver;
use epiphany_layout_ir::{BoundingBox, HitShape, LayoutObjectId, Point};
use epiphany_ops::{OperationKind, OperationPayload};
use epiphany_render_svg::{render, RenderOptions};

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Epiphany editor (demo)",
        options,
        Box::new(|_cc| Ok(Box::new(EditorApp::new()))),
    )
}

/// Maps between staff-space **world** coordinates (the hit-test map's space) and
/// on-screen points, given where the rendered image is currently drawn.
///
/// `epiphany-render-svg` lays the score out in a viewBox of `view_box = [min_x,
/// min_y, width, height]` staff spaces and draws it under a single
/// `translate(-min_x, max_y) scale(1 -1)` group (world is y-up, screen is y-down),
/// where `max_y = min_y + height`. Displaying that image in `rect` therefore makes
/// the world↔screen transform a uniform scale + flip, which this inverts.
struct ViewMap {
    min_x: f32,
    max_y: f32,
    vb_width: f32,
    rect: egui::Rect,
}

impl ViewMap {
    fn new(view_box: [f32; 4], rect: egui::Rect) -> Self {
        ViewMap {
            min_x: view_box[0],
            max_y: view_box[1] + view_box[3],
            vb_width: view_box[2],
            rect,
        }
    }

    /// On-screen points per staff space, derived from the displayed rect (so it is
    /// correct at any zoom/fit). Uniform: the image keeps the layout's aspect ratio.
    fn scale(&self) -> f32 {
        if self.vb_width > 0.0 {
            self.rect.width() / self.vb_width
        } else {
            1.0
        }
    }

    fn screen_to_world(&self, p: egui::Pos2) -> Point {
        let s = self.scale();
        Point::new(
            self.min_x + (p.x - self.rect.min.x) / s,
            self.max_y - (p.y - self.rect.min.y) / s,
        )
    }

    fn world_to_screen(&self, x: f32, y: f32) -> egui::Pos2 {
        let s = self.scale();
        egui::pos2(
            self.rect.min.x + (x - self.min_x) * s,
            self.rect.min.y + (self.max_y - y) * s,
        )
    }
}

/// The screen rectangle covering a hit shape, for the selection highlight.
fn shape_rect(shape: &HitShape, vm: &ViewMap) -> egui::Rect {
    match shape {
        HitShape::Box(BoundingBox {
            left,
            bottom,
            right,
            top,
        }) => egui::Rect::from_two_pos(
            vm.world_to_screen(left.0, top.0),
            vm.world_to_screen(right.0, bottom.0),
        ),
        HitShape::Segment { from, to, .. } => egui::Rect::from_two_pos(
            vm.world_to_screen(from.x.0, from.y.0),
            vm.world_to_screen(to.x.0, to.y.0),
        ),
        // A curve's highlight rectangle is its control-point hull (the box that
        // bounds the drawn arc), mapped to screen space.
        HitShape::Curve { p0, p1, p2, p3, .. } => {
            let xs = [p0.x.0, p1.x.0, p2.x.0, p3.x.0];
            let ys = [p0.y.0, p1.y.0, p2.y.0, p3.y.0];
            let min = |a: &[f32]| a.iter().copied().fold(f32::INFINITY, f32::min);
            let max = |a: &[f32]| a.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            egui::Rect::from_two_pos(
                vm.world_to_screen(min(&xs), max(&ys)),
                vm.world_to_screen(max(&xs), min(&ys)),
            )
        }
    }
}

/// The screen rect for the hit-test region backing `layout_object`, if it is
/// still present in the current layout — shared by the selection overlay's
/// member and anchor passes.
fn selection_rect(
    session: &EditorSession,
    layout_object: LayoutObjectId,
    vm: &ViewMap,
) -> Option<egui::Rect> {
    session
        .hit_test()
        .regions
        .iter()
        .find(|r| r.layout_object == layout_object)
        .map(|region| shape_rect(&region.shape, vm))
}

/// Accumulates a rubber-band drag's two screen-space corners (the point where
/// the drag started and where the pointer currently is), pure and independent
/// of any `egui::Response`/`Context` so it can be unit-tested headlessly. The
/// *world*-space selection query ([`EditorSession::select_within`]) maps
/// `origin`/`current` directly and lets that API normalize corner order
/// itself (its own doc: "rect's corners are order-independent"); this
/// struct's own min/max normalization ([`Self::screen_rect`]) exists only so
/// the live overlay always paints a valid (non-inverted) screen rectangle,
/// regardless of which direction the drag went.
#[derive(Clone, Copy, Debug, PartialEq)]
struct DragRect {
    origin: egui::Pos2,
    current: egui::Pos2,
}

impl DragRect {
    /// Below this screen-space distance (points), a completed drag still
    /// resolves as a plain click/toggle rather than a rubber-band select.
    /// Chosen smaller than egui's own default click-distance tolerance
    /// (`InputOptions::max_click_dist`, 6.0 points): egui reclassifies a
    /// long, still press-and-hold as "dragging" purely by elapsed time (see
    /// `PointerState::is_decidedly_dragging`), even with ~0 pointer movement,
    /// so a widget sensing both click and drag can receive
    /// `drag_started`/`drag_stopped` for a gesture a user experiences as a
    /// click. This threshold exists to catch exactly that case and resolve
    /// it as a click, not a near-empty rubber-band replace.
    const RUBBER_BAND_THRESHOLD: f32 = 4.0;

    fn new(origin: egui::Pos2) -> Self {
        DragRect {
            origin,
            current: origin,
        }
    }

    fn update(&mut self, pos: egui::Pos2) {
        self.current = pos;
    }

    /// The screen-space distance travelled from the drag's origin so far.
    fn distance(&self) -> f32 {
        self.origin.distance(self.current)
    }

    /// Whether the drag has moved far enough to count as a rubber-band
    /// select rather than a click (see [`Self::RUBBER_BAND_THRESHOLD`]).
    fn is_rubber_band(&self) -> bool {
        self.distance() >= Self::RUBBER_BAND_THRESHOLD
    }

    /// The normalized on-screen rectangle spanning the drag so far — `min <=
    /// max` on both axes regardless of which corner the drag started from.
    /// The rubber-band overlay's paint rect.
    fn screen_rect(&self) -> egui::Rect {
        let min_x = self.origin.x.min(self.current.x);
        let max_x = self.origin.x.max(self.current.x);
        let min_y = self.origin.y.min(self.current.y);
        let max_y = self.origin.y.max(self.current.y);
        egui::Rect::from_min_max(egui::pos2(min_x, min_y), egui::pos2(max_x, max_y))
    }
}

/// What a drag's release should do, decided purely from the accumulated
/// screen-space drag and whether Ctrl/Cmd was held at release — split out
/// from the release handler itself so the threshold decision (rubber-band
/// vs. click/toggle) has a headlessly testable seam.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReleaseAction {
    /// At or above the rubber-band threshold: `select_within` over the
    /// drag's rectangle.
    RubberBand,
    /// Below the threshold: resolves as a click (`toggle` = Ctrl/Cmd was
    /// held, so toggle membership instead of replacing the selection).
    Point { toggle: bool },
}

fn resolve_release(drag: &DragRect, ctrl: bool) -> ReleaseAction {
    if drag.is_rubber_band() {
        ReleaseAction::RubberBand
    } else {
        ReleaseAction::Point { toggle: ctrl }
    }
}

/// A short label for the last applied op, for the debug panel.
fn payload_label(payload: &OperationPayload) -> &'static str {
    match payload {
        OperationPayload::Primitive(kind) => match kind {
            OperationKind::Transpose(_) => "Transpose",
            OperationKind::TransposeInterval(_) => "TransposeInterval",
            OperationKind::ModifyIdentifiedPitch(_) => "ModifyIdentifiedPitch",
            OperationKind::DeleteIdentifiedPitch(_) => "DeleteIdentifiedPitch",
            OperationKind::DeleteEvent(_) => "DeleteEvent",
            OperationKind::InsertIdentifiedPitch(_) => "InsertIdentifiedPitch",
            OperationKind::InsertEvent(_) => "InsertEvent",
            OperationKind::RespellPitch(_) => "RespellPitch",
            OperationKind::DeclareTransaction(_) => "DeclareTransaction",
            OperationKind::CreateStaff(_) => "CreateStaff",
            OperationKind::SetTimeSignature(_) => "SetTimeSignature",
            OperationKind::SetTempoSegment(_) => "SetTempoSegment",
            OperationKind::SetStaffLayout(_) => "SetStaffLayout",
            OperationKind::CreateRepeatStructure(_) => "CreateRepeatStructure",
            OperationKind::DeleteRepeatStructure(_) => "DeleteRepeatStructure",
            _ => "primitive",
        },
        OperationPayload::ResolveConflict(_) => "ResolveConflict",
        OperationPayload::UndoTransaction(_) => "UndoTransaction",
        OperationPayload::ResolveEquivocation(_) => "ResolveEquivocation",
    }
}

/// Rasterizes a rendered SVG string to a `tiny_skia` pixmap, returning it and the
/// SVG's **logical** (sub-pixel, pre-`ceil`) size. The pixmap dimensions are the
/// logical size rounded up to whole pixels; the logical size is what the image must
/// be *displayed* at so the click plane maps back to the layout exactly (displaying
/// the rounded-up pixmap size would stretch the mapping past the content). The score
/// is drawn over an opaque white background so the pixmap's premultiplied alpha is
/// fully opaque, matching `from_rgba_unmultiplied` (this crate's only consumer of the
/// pixmap's raw bytes, [`rasterize`], and the golden-image tests in `goldens.rs`,
/// which lock this exact pixmap — the surface the GUI displays).
fn rasterize_pixmap(svg: &str) -> Option<(resvg::tiny_skia::Pixmap, egui::Vec2)> {
    let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default()).ok()?;
    let size = tree.size();
    let logical = egui::vec2(size.width(), size.height());
    let width = size.width().ceil().max(1.0) as u32;
    let height = size.height().ceil().max(1.0) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)?;
    pixmap.fill(resvg::tiny_skia::Color::WHITE);
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    Some((pixmap, logical))
}

/// Rasterizes a rendered SVG string to an `egui` image (the GUI's display path): a
/// thin conversion over [`rasterize_pixmap`]'s pixmap, see its doc for the sizing and
/// background contract.
fn rasterize(svg: &str) -> Option<(egui::ColorImage, egui::Vec2)> {
    let (pixmap, logical) = rasterize_pixmap(svg)?;
    let image = egui::ColorImage::from_rgba_unmultiplied(
        [pixmap.width() as usize, pixmap.height() as usize],
        pixmap.data(),
    );
    Some((image, logical))
}

struct EditorApp {
    session: EditorSession,
    texture: Option<egui::TextureHandle>,
    /// `[min_x, min_y, width, height]` of the last render, in staff spaces.
    view_box: [f32; 4],
    /// The logical (sub-pixel) on-screen size of the last render, in points — what
    /// the texture is displayed at (its pixmap is this rounded up). Kept in step with
    /// `texture` / `view_box` so the click plane matches the displayed content.
    logical_size: egui::Vec2,
    px_per_staff_space: f32,
    needs_render: bool,
    /// Pencil ("insert") mode: a click on the staff inserts a note at that pitch and
    /// beat (with make-room overwrite) instead of selecting.
    pencil: bool,
    /// The in-progress rubber-band drag, if any (`None` outside a drag, and always
    /// `None` in pencil mode — pencil's click-to-insert takes precedence and ignores
    /// dragging entirely).
    rubber_band: Option<DragRect>,
    status: String,
}

impl EditorApp {
    fn new() -> Self {
        let score = epiphany_testkit::fixtures::ten_measure_single_staff(0);
        let session = EditorSession::open(score, Box::new(Engraver::default()))
            .expect("the ten-measure fixture renders under the real engraver");
        EditorApp {
            session,
            texture: None,
            view_box: [0.0, 0.0, 1.0, 1.0],
            logical_size: egui::vec2(1.0, 1.0),
            px_per_staff_space: 12.0,
            needs_render: true,
            pencil: false,
            rubber_band: None,
            status: "opened ten_measure_single_staff".to_string(),
        }
    }

    /// Runs an intent, recording its outcome (or error) in the status line and
    /// scheduling a re-render.
    fn run(
        &mut self,
        name: &str,
        intent: impl FnOnce(&mut EditorSession) -> Result<EditOutcome, EditorError>,
    ) {
        self.status = match intent(&mut self.session) {
            Ok(outcome) => format!(
                "{name}: ok (changed={}, selection kept={})",
                outcome.graph_changed, outcome.selection_preserved
            ),
            Err(err) => format!("{name}: {err}"),
        };
        self.needs_render = true;
    }

    /// Runs an undo/redo step (which returns `None` when there is nothing to do),
    /// recording the outcome in the status line and scheduling a re-render.
    fn run_history(
        &mut self,
        name: &str,
        step: impl FnOnce(&mut EditorSession) -> Option<EditOutcome>,
    ) {
        self.status = match step(&mut self.session) {
            Some(outcome) => format!("{name}: ok (changed={})", outcome.graph_changed),
            None => format!("nothing to {name}"),
        };
        self.needs_render = true;
    }

    /// Re-renders the session's resolved layout to a texture. The view box, logical
    /// size, and texture are updated together (only on a successful rasterization), so
    /// the displayed pixels never disagree with the click plane; on failure the score
    /// view is cleared and the failure is surfaced rather than left stale.
    fn rerender(&mut self, ctx: &egui::Context) {
        let options = RenderOptions {
            px_per_staff_space: self.px_per_staff_space,
            ..Default::default()
        };
        let output = render(self.session.resolved(), &options);
        match rasterize(&output.svg) {
            Some((image, logical)) => {
                self.view_box = output.stats.view_box;
                self.logical_size = logical;
                self.texture = Some(ctx.load_texture("score", image, egui::TextureOptions::LINEAR));
            }
            None => {
                self.texture = None;
                self.status = "render failed: could not rasterize the score".to_string();
            }
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(self.session.can_undo(), egui::Button::new("↶ Undo"))
                .clicked()
            {
                self.run_history("undo", |s| s.undo());
            }
            if ui
                .add_enabled(self.session.can_redo(), egui::Button::new("↷ Redo"))
                .clicked()
            {
                self.run_history("redo", |s| s.redo());
            }
            ui.separator();
            if ui.button("Delete").clicked() {
                self.run("delete", |s| s.delete_selection());
            }
            if ui.button("Transpose ♯").clicked() {
                self.run("transpose +1", |s| s.alter_selection(1));
            }
            if ui.button("Transpose ♭").clicked() {
                self.run("transpose -1", |s| s.alter_selection(-1));
            }
            if ui.button("Move ↑").clicked() {
                self.run("move up", |s| s.move_selection_staff_step(1));
            }
            if ui.button("Move ↓").clicked() {
                self.run("move down", |s| s.move_selection_staff_step(-1));
            }
            if ui.button("Add chord note").clicked() {
                self.run("add chord note", |s| s.add_note_to_selection());
            }
            if ui.button("Insert after").clicked() {
                self.run("insert after", |s| s.insert_note_after_selection());
            }
            ui.separator();
            // Duration palette: set the selected note/rest's written value (make-room
            // overwrite when lengthening).
            ui.label("Dur:");
            for (label, value) in [
                ("1", NoteValue::Whole),
                ("1/2", NoteValue::Half),
                ("1/4", NoteValue::Quarter),
                ("1/8", NoteValue::Eighth),
                ("1/16", NoteValue::Sixteenth),
            ] {
                if ui.button(label).clicked() {
                    self.run(&format!("duration {label}"), |s| {
                        s.set_selection_duration(value.whole_note_fraction())
                    });
                }
            }
            ui.separator();
            ui.toggle_value(&mut self.pencil, "✏ Pencil (insert)");
            ui.separator();
            if ui
                .add(egui::Slider::new(&mut self.px_per_staff_space, 6.0..=28.0).text("zoom"))
                .changed()
            {
                self.needs_render = true;
            }
        });
    }

    fn debug_panel(&self, ui: &mut egui::Ui) {
        ui.heading("State");
        ui.label(format!("status: {}", self.status));
        ui.separator();
        ui.label(format!(
            "selected: {} member(s)",
            self.session.selections().len()
        ));
        match self.session.anchor() {
            Some(sel) => {
                ui.label(format!("anchor source: {:?}", sel.source));
                ui.label(format!("anchor layout id: {:?}", sel.layout_object));
            }
            None => {
                ui.label("selection: none");
            }
        }
        ui.separator();
        ui.label(format!(
            "ops applied: {}",
            self.session.applied_operations().len()
        ));
        match self.session.last_applied() {
            Some(env) => {
                ui.label(format!("last op: {}", payload_label(&env.payload)));
                ui.label(format!("op id: {:?}", env.id));
            }
            None => {
                ui.label("last op: none");
            }
        }
        ui.separator();
        ui.label(format!(
            "Click a notehead to select (replaces). Ctrl/Cmd-click toggles a member. Drag a \
             rubber-band to select every hit within it. Pencil mode: {}.\n\
             Delete and Transpose act on the whole selection; Move / Add chord / Insert after / \
             duration act on the anchor (the drag/toggle reference point, drawn with the \
             thicker accent stroke).\n\
             Keys: Del delete · ↑/↓ staff-step move · +/− transpose · A add chord · I insert after · P pencil · Ctrl/Cmd+Z undo · Ctrl/Cmd+Shift+Z or Ctrl/Cmd+Y redo",
            if self.pencil { "ON — click to insert" } else { "off" }
        ));
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        let k = ctx.input(|i| Keys {
            // Ctrl/Cmd-Z undo, Ctrl/Cmd-Shift-Z (or Ctrl/Cmd-Y) redo, read first so a
            // modified Z is not also taken as a plain key.
            undo: i.modifiers.command && !i.modifiers.shift && i.key_pressed(egui::Key::Z),
            redo: i.modifiers.command
                && ((i.modifiers.shift && i.key_pressed(egui::Key::Z))
                    || i.key_pressed(egui::Key::Y)),
            delete: i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace),
            move_up: i.key_pressed(egui::Key::ArrowUp),
            move_down: i.key_pressed(egui::Key::ArrowDown),
            sharp: i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals),
            flat: i.key_pressed(egui::Key::Minus),
            add: i.key_pressed(egui::Key::A),
            insert: i.key_pressed(egui::Key::I),
            pencil: i.key_pressed(egui::Key::P),
        });
        if k.undo {
            self.run_history("undo", |s| s.undo());
        }
        if k.redo {
            self.run_history("redo", |s| s.redo());
        }
        if k.pencil {
            self.pencil = !self.pencil;
            self.status = format!("pencil mode {}", if self.pencil { "on" } else { "off" });
        }
        if k.delete {
            self.run("delete", |s| s.delete_selection());
        }
        if k.move_up {
            self.run("move up", |s| s.move_selection_staff_step(1));
        }
        if k.move_down {
            self.run("move down", |s| s.move_selection_staff_step(-1));
        }
        if k.sharp {
            self.run("transpose +1", |s| s.alter_selection(1));
        }
        if k.flat {
            self.run("transpose -1", |s| s.alter_selection(-1));
        }
        if k.add {
            self.run("add chord note", |s| s.add_note_to_selection());
        }
        if k.insert {
            self.run("insert after", |s| s.insert_note_after_selection());
        }
    }

    /// Resolves a non-pencil click/toggle at world `point`: Ctrl/Cmd held (`toggle`)
    /// adds/removes it from the set ([`EditorSession::toggle_at`]); otherwise it
    /// replaces the selection ([`EditorSession::click`], today's exact behavior),
    /// reporting on empty space what a pencil click *would* insert. Shared by the
    /// plain-click path and the release path for a drag that stayed under the
    /// rubber-band threshold, so both resolve identically.
    fn resolve_click(&mut self, world: Point, grid: &GridResolution, toggle: bool) {
        if toggle {
            let members = self.session.toggle_at(world);
            self.status = format!("toggled — {} member(s) selected", members.len());
            return;
        }
        // Select the topmost hit; on empty staff, report what a pencil click
        // *would* insert (the pitch and grid-snapped beat).
        let pitch = self.session.staff_pitch_at(world);
        let position = self.session.position_at(world, grid);
        self.status = match self.session.click(world) {
            Some(_) => "selected".to_string(),
            None => match (pitch, position) {
                (Some(p), Some(gp)) => format!(
                    "empty — pencil would insert {:?}{} at beat {:.3}",
                    p.nominal,
                    p.octave,
                    gp.position.0.to_f64()
                ),
                (Some(p), None) => {
                    format!("empty — pencil would insert {:?}{}", p.nominal, p.octave)
                }
                _ => "cleared selection".to_string(),
            },
        };
    }

    fn score_view(&mut self, ui: &mut egui::Ui) {
        let Some(texture) = self.texture.clone() else {
            return;
        };
        // Display at the logical size (not the rounded-up pixmap size), cropping the
        // texture's uv to the logical content — so the click plane maps screen points
        // back to the layout exactly, with no sub-pixel stretch from the `ceil`.
        let pixmap = texture.size_vec2();
        let logical = self.logical_size;
        let (rect, response) = ui.allocate_exact_size(logical, egui::Sense::click_and_drag());
        let uv = egui::Rect::from_min_max(
            egui::pos2(0.0, 0.0),
            egui::pos2(logical.x / pixmap.x, logical.y / pixmap.y),
        );
        ui.painter()
            .image(texture.id(), rect, uv, egui::Color32::WHITE);

        let vm = ViewMap::new(self.view_box, rect);

        // Rubber-band drag tracking. Pencil mode ignores dragging entirely — it takes
        // precedence, and its click-to-insert behavior below is unchanged — so no drag
        // is ever started while it is on.
        if !self.pencil {
            if response.drag_started() {
                if let Some(pos) = response.interact_pointer_pos() {
                    self.rubber_band = Some(DragRect::new(pos));
                }
            } else if response.dragged() {
                if let Some(pos) = response.interact_pointer_pos() {
                    match self.rubber_band.as_mut() {
                        Some(drag) => drag.update(pos),
                        // `dragged` without a prior `drag_started` shouldn't happen, but
                        // start tracking now rather than lose the gesture.
                        None => self.rubber_band = Some(DragRect::new(pos)),
                    }
                }
            }
            if response.drag_stopped() {
                if let Some(mut drag) = self.rubber_band.take() {
                    // `dragged()` is already false on the release frame (egui clears it
                    // before `drag_stopped` fires), but `interact_pointer_pos` still
                    // reports the release position — fold it in so the rect's second
                    // corner is the exact release point, not the prior frame's.
                    if let Some(pos) = response.interact_pointer_pos() {
                        drag.update(pos);
                    }
                    let ctrl = ui.input(|i| i.modifiers.command);
                    match resolve_release(&drag, ctrl) {
                        ReleaseAction::RubberBand => {
                            // `select_within`'s `BoundingBox` normalizes corner order
                            // itself, so the two raw corners are passed through as-is.
                            let a = vm.screen_to_world(drag.origin);
                            let b = vm.screen_to_world(drag.current);
                            let members = self
                                .session
                                .select_within(BoundingBox::new(a.x.0, a.y.0, b.x.0, b.y.0));
                            self.status = format!("selected {} member(s)", members.len());
                        }
                        ReleaseAction::Point { toggle } => {
                            let world = vm.screen_to_world(drag.current);
                            let grid = self
                                .session
                                .default_grid_at(world)
                                .unwrap_or_else(GridResolution::quarter);
                            self.resolve_click(world, &grid, toggle);
                        }
                    }
                }
            }
        }

        // Live rubber-band overlay while dragging.
        if let Some(drag) = &self.rubber_band {
            ui.painter().rect(
                drag.screen_rect(),
                0.0,
                egui::Color32::from_rgba_unmultiplied(0, 120, 215, 40),
                egui::Stroke::new(
                    1.5_f32,
                    egui::Color32::from_rgba_unmultiplied(0, 120, 215, 180),
                ),
            );
        }

        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let world = vm.screen_to_world(pos);
                // The grid the pencil snaps to: the meter's beat under the cursor,
                // defaulting to a quarter off-staff.
                let grid = self
                    .session
                    .default_grid_at(world)
                    .unwrap_or_else(GridResolution::quarter);
                if self.pencil {
                    // Insert a note at the clicked pitch/beat, making room over whatever
                    // it overlaps. This runs *after* this frame's rerender slot, so
                    // request a repaint to draw the edit, and return before the selection
                    // overlay below paints the new hit map over the old (not-yet-
                    // rerendered) texture.
                    self.run("insert note", |s| s.insert_note_at(world, &grid));
                    ui.ctx().request_repaint();
                    return;
                } else {
                    let ctrl = ui.input(|i| i.modifiers.command);
                    self.resolve_click(world, &grid, ctrl);
                }
            }
        }

        // Selection overlay: every member gets the existing highlight; the anchor
        // additionally gets a thicker, differently-colored accent stroke on top (for a
        // single selection the sole member is trivially the anchor, so it gets both).
        for sel in self.session.selections() {
            if let Some(rc) = selection_rect(&self.session, sel.layout_object, &vm) {
                ui.painter().rect_stroke(
                    rc,
                    0.0,
                    // Suffixed: `float_literal_f32_fallback` (Rust 1.97) rejects
                    // inferring `f32` here, and it becomes a hard error later.
                    egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(0, 120, 215)),
                );
            }
        }
        if let Some(anchor) = self.session.anchor() {
            if let Some(rc) = selection_rect(&self.session, anchor.layout_object, &vm) {
                ui.painter().rect_stroke(
                    rc,
                    0.0,
                    egui::Stroke::new(3.0_f32, egui::Color32::from_rgb(255, 140, 0)),
                );
            }
        }
    }
}

/// Keyboard edges read once per frame (so a held key fires one edit).
struct Keys {
    undo: bool,
    redo: bool,
    delete: bool,
    move_up: bool,
    move_down: bool,
    sharp: bool,
    flat: bool,
    add: bool,
    insert: bool,
    pencil: bool,
}

impl eframe::App for EditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_keys(ctx);

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| self.toolbar(ui));
        egui::SidePanel::right("debug")
            .default_width(280.0)
            .show(ctx, |ui| self.debug_panel(ui));

        if self.needs_render {
            self.rerender(ctx);
            self.needs_render = false;
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::both().show(ui, |ui| self.score_view(ui));
        });
    }
}

// The pixel-golden comparator, bless machinery, and the four golden-state tests
// (T1a); see `goldens.rs`'s module doc. Test-only: never compiled into the
// shipped binary.
#[cfg(test)]
mod goldens;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_map_round_trips_screen_and_world() {
        // A viewBox offset in both axes (min_y negative), shown in an off-origin rect.
        let view_box = [3.0_f32, -5.0, 40.0, 12.0];
        let rect = egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(400.0, 120.0));
        let vm = ViewMap::new(view_box, rect);

        for &(sx, sy) in &[(100.0, 50.0), (500.0, 170.0), (300.0, 110.0), (250.0, 75.0)] {
            let world = vm.screen_to_world(egui::pos2(sx, sy));
            let back = vm.world_to_screen(world.x.0, world.y.0);
            assert!(
                (back.x - sx).abs() < 1e-3,
                "x round-trips: {} vs {sx}",
                back.x
            );
            assert!(
                (back.y - sy).abs() < 1e-3,
                "y round-trips: {} vs {sy}",
                back.y
            );
        }
    }

    #[test]
    fn click_plane_uses_the_logical_not_ceiled_size() {
        // 82.4564 staff spaces at 12 px/space is 989.4768 logical px, which a pixmap
        // rounds up to 990. The click plane must map the *logical* rect to the
        // viewBox, so the scale is exactly px-per-staff-space — using the rounded-up
        // pixmap size instead would stretch it.
        let view_box = [0.0_f32, 0.0, 82.4564, 30.0];
        let pps = 12.0_f32;
        let logical = egui::vec2(view_box[2] * pps, view_box[3] * pps);

        let logical_rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), logical);
        let correct = ViewMap::new(view_box, logical_rect);
        assert!(
            (correct.scale() - pps).abs() < 1e-3,
            "logical rect gives scale == px/space, got {}",
            correct.scale()
        );

        // The rounded-up pixmap size must NOT be used as the display rect — a
        // regression guard that it would give a different (wrong) scale.
        let ceiled = egui::vec2(logical.x.ceil(), logical.y.ceil());
        let ceiled_rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), ceiled);
        assert!(
            (ViewMap::new(view_box, ceiled_rect).scale() - pps).abs() > 1e-3,
            "the rounded-up size would stretch the click plane"
        );
    }

    #[test]
    fn view_map_corners_respect_the_y_up_world() {
        let view_box = [3.0_f32, -5.0, 40.0, 12.0]; // max_y = -5 + 12 = 7
        let rect = egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(400.0, 120.0));
        let vm = ViewMap::new(view_box, rect);

        // Screen top-left is world (min_x, max_y); screen bottom-left is (min_x, min_y).
        let top_left = vm.screen_to_world(rect.min);
        assert!((top_left.x.0 - 3.0).abs() < 1e-3);
        assert!((top_left.y.0 - 7.0).abs() < 1e-3);
        let bottom_left = vm.screen_to_world(egui::pos2(rect.min.x, rect.max.y));
        assert!((bottom_left.y.0 - (-5.0)).abs() < 1e-3);
    }

    #[test]
    fn drag_rect_screen_rect_normalizes_any_corner_order() {
        // Dragging bottom-right -> top-left must paint the same rect as top-left ->
        // bottom-right: `screen_rect` normalizes min<=max on both axes regardless of
        // which corner the drag started from. A corner-math mutation (e.g. swapping
        // which side takes `.min` vs `.max`) makes one of these directions produce an
        // inverted (or simply wrong) rect, which this equality catches.
        let mut forward = DragRect::new(egui::pos2(10.0, 20.0));
        forward.update(egui::pos2(50.0, 80.0));
        let mut backward = DragRect::new(egui::pos2(50.0, 80.0));
        backward.update(egui::pos2(10.0, 20.0));

        let expected = egui::Rect::from_min_max(egui::pos2(10.0, 20.0), egui::pos2(50.0, 80.0));
        assert_eq!(forward.screen_rect(), expected);
        assert_eq!(backward.screen_rect(), expected);

        // Every other corner order too, so a mutation on just one axis's min/max still
        // gets caught.
        let mut mixed = DragRect::new(egui::pos2(50.0, 20.0));
        mixed.update(egui::pos2(10.0, 80.0));
        assert_eq!(mixed.screen_rect(), expected);
    }

    #[test]
    fn drag_rect_distance_is_euclidean() {
        let mut drag = DragRect::new(egui::pos2(0.0, 0.0));
        drag.update(egui::pos2(3.0, 4.0));
        assert!((drag.distance() - 5.0).abs() < 1e-5);
    }

    #[test]
    fn drag_rect_threshold_distinguishes_click_from_rubber_band() {
        // Below the threshold: still a click.
        let mut small = DragRect::new(egui::pos2(0.0, 0.0));
        small.update(egui::pos2(1.0, 1.0));
        assert!(small.distance() < DragRect::RUBBER_BAND_THRESHOLD);
        assert!(!small.is_rubber_band());

        // At the threshold exactly: counts as a rubber-band (>=).
        let mut at_threshold = DragRect::new(egui::pos2(0.0, 0.0));
        at_threshold.update(egui::pos2(DragRect::RUBBER_BAND_THRESHOLD, 0.0));
        assert!(at_threshold.is_rubber_band());

        // Well above: a rubber-band.
        let mut large = DragRect::new(egui::pos2(0.0, 0.0));
        large.update(egui::pos2(100.0, 0.0));
        assert!(large.is_rubber_band());
    }

    #[test]
    fn resolve_release_below_threshold_is_a_point_carrying_the_modifier() {
        let mut drag = DragRect::new(egui::pos2(0.0, 0.0));
        drag.update(egui::pos2(1.0, 1.0));
        assert_eq!(
            resolve_release(&drag, false),
            ReleaseAction::Point { toggle: false }
        );
        assert_eq!(
            resolve_release(&drag, true),
            ReleaseAction::Point { toggle: true }
        );
    }

    #[test]
    fn resolve_release_at_or_above_threshold_is_rubber_band_regardless_of_modifier() {
        // This is mutation w2b's headlessly-testable seam: a release handler that
        // dispatched `Point`/`click` here instead of `RubberBand`/`select_within` would
        // fail this assertion outright.
        let mut drag = DragRect::new(egui::pos2(0.0, 0.0));
        drag.update(egui::pos2(100.0, 0.0));
        assert_eq!(resolve_release(&drag, false), ReleaseAction::RubberBand);
        assert_eq!(resolve_release(&drag, true), ReleaseAction::RubberBand);
    }
}
