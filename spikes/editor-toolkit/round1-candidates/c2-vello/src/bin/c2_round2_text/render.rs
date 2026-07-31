//! Check 1 (faithful consumption) + check 2's rendering half: offscreen vello
//! rendering of a `SpikeResolvedText`, drawing exactly the resolved
//! `(face, glyph_id, offset)` triples — no text layout API, no font
//! fallback, no rustybuzz.
//!
//! **The candidate-owned part named in the task instructions**: outline
//! extraction from the face and conversion to a `kurbo::BezPath`. This module
//! implements `ttf_parser::OutlineBuilder` directly, the same way
//! `round2-svgref` does for its own (SVG-string, reference-only) output — but
//! that crate is deliberately not depended on here: its output type is a
//! `<path d="...">` string for the reference emitter, not `kurbo` geometry
//! for a candidate to feed `vello::Scene::fill`.

use anyhow::{anyhow, Result};
use ttf_parser::{Face as TtfFace, GlyphId, OutlineBuilder};
use vello::kurbo::{Affine, BezPath, Point as KPoint};
use vello::peniko::{color::palette, Color, Fill};
use vello::wgpu;
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};

use round2_candidatekit::inputs::{HEIGHT, WIDTH};
use round2_textkit::faces::LoadedFace;
use round2_textkit::hittest::to_device;
use round2_textkit::types::SpikeResolvedText;
use round2_textkit::DEVICE_SCALE;

/// Opaque black ink on an opaque white ground (recipe §3/§10: "verified:
/// every reference pixel has alpha 255, ground is #ffffff, ink is #000000"),
/// the same convention Round 1 used.
const INK: Color = palette::css::BLACK;
const GROUND: Color = palette::css::WHITE;

/// vello's `render_to_texture` requires `Rgba8Unorm` + `STORAGE_BINDING` —
/// same literal deviation from pin 4's "sRGB target format" Round 1's C2
/// documented and for the same reason; immaterial here since `round2_diff`
/// compares luma classifications, not raw channel values under a transfer
/// function.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Same AA config Round 1's C2 used, so this packet's choice is traceable to
/// that precedent rather than picked fresh; Round 2's check 1 is a bounded
/// visual differential against a reference raster (recipe §10), not a
/// pixel-exact comparison, so the AA method does not change the outcome the
/// way it would in Round 4's timings.
const AA: AaConfig = AaConfig::Msaa8;

pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub renderer: Renderer,
    pub adapter_name: String,
    pub adapter_device_type: String,
}

/// Initializes one headless wgpu device + vello renderer, reused across all
/// five fixtures. Prefers the integrated adapter (pin 4: "the integrated
/// adapter's figures decide"), falling back to the first enumerated Vulkan
/// adapter — Round 2's check 1 is a correctness check, not the adapter-class
/// comparison pin 4 requires for Round 4's timings, so a single adapter is
/// sufficient here and the one chosen is recorded in the report.
pub fn init_gpu() -> Result<Gpu> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::VULKAN));
    if adapters.is_empty() {
        return Err(anyhow!(
            "NOT RUN: no Vulkan adapters enumerated — environment absence, not a candidate defect"
        ));
    }
    let adapter = adapters
        .iter()
        .find(|a| a.get_info().device_type == wgpu::DeviceType::IntegratedGpu)
        .unwrap_or(&adapters[0]);
    let info = adapter.get_info();

    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("c2-vello-round2-text"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::default(),
        trace: wgpu::Trace::Off,
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
    }))?;

    let renderer = Renderer::new(
        &device,
        RendererOptions {
            use_cpu: false,
            antialiasing_support: AaSupport::all(),
            num_init_threads: None,
            pipeline_cache: None,
        },
    )
    .map_err(|e| anyhow!("vello Renderer::new failed: {e}"))?;

    Ok(Gpu {
        device,
        queue,
        renderer,
        adapter_name: info.name.clone(),
        adapter_device_type: format!("{:?}", info.device_type),
    })
}

/// Collects one glyph's outline into a device-space `kurbo::BezPath`,
/// scaling by `em_px / units_per_em` and flipping y (font space is y-up,
/// device space is y-down) — the same rule Round 1's `build_path` used for
/// Bravura outlines, applied here to a `ttf_parser` face outline instead of
/// a typed `PathCommand` sequence.
///
/// Returns `None` for a glyph with no outline (whitespace, or — under W3-F3's
/// invariant — a glyph id that does not exist in this face at all, which
/// never happens here because `seg.face` is only ever `Some` when resolution
/// found real coverage).
fn build_glyph_bezpath(
    face: &TtfFace,
    glyph_id: u16,
    em_px: f64,
    origin: KPoint,
) -> Option<BezPath> {
    struct Sink {
        path: BezPath,
        scale: f64,
        ox: f64,
        oy: f64,
        open: bool,
        any: bool,
    }
    impl Sink {
        fn map(&self, x: f32, y: f32) -> KPoint {
            KPoint::new(
                self.ox + x as f64 * self.scale,
                self.oy - y as f64 * self.scale,
            )
        }
    }
    impl OutlineBuilder for Sink {
        fn move_to(&mut self, x: f32, y: f32) {
            if self.open {
                self.path.close_path();
            }
            let p = self.map(x, y);
            self.path.move_to(p);
            self.open = true;
            self.any = true;
        }
        fn line_to(&mut self, x: f32, y: f32) {
            self.path.line_to(self.map(x, y));
        }
        fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
            self.path.quad_to(self.map(x1, y1), self.map(x, y));
        }
        fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
            self.path
                .curve_to(self.map(x1, y1), self.map(x2, y2), self.map(x, y));
        }
        fn close(&mut self) {
            self.path.close_path();
            self.open = false;
        }
    }

    let upem = face.units_per_em() as f64;
    if upem <= 0.0 {
        return None;
    }
    let mut sink = Sink {
        path: BezPath::new(),
        scale: em_px / upem,
        ox: origin.x,
        oy: origin.y,
        open: false,
        any: false,
    };
    face.outline_glyph(GlyphId(glyph_id), &mut sink)?;
    if sink.open {
        sink.path.close_path();
    }
    if !sink.any {
        return None;
    }
    Some(sink.path)
}

/// Builds one `vello::Scene` for `rt`, drawing exactly the resolved
/// `(face, glyph_id, offset)` triples — **never re-shaping**. A segment whose
/// `face` is `None` (F-C's uncovered Arabic letter) has no glyphs by
/// construction (W3-F3 / `invariants::assert_unresolved_clusters_are_diagnostic`,
/// asserted on every loaded fixture by `FixtureFile::validate`), so the loop
/// below draws nothing for it and substitutes nothing — there is no
/// "draw `.notdef`" branch to suppress because shaping was never attempted
/// against a face that does not cover the codepoint.
fn build_scene(rt: &SpikeResolvedText, faces: &[LoadedFace]) -> Result<Scene> {
    let mut scene = Scene::new();
    for (seg_idx, seg) in rt.segments.iter().enumerate() {
        let Some(face_idx) = seg.face else {
            // Uncovered span: `seg.glyphs` is guaranteed empty here. Nothing
            // drawn, nothing substituted.
            continue;
        };
        let loaded = faces.get(face_idx as usize).ok_or_else(|| {
            anyhow!(
                "segment {seg_idx} resolved to face {face_idx}, but only {} faces are loaded",
                faces.len()
            )
        })?;
        let face = TtfFace::parse(&loaded.bytes, loaded.identity.face_index)
            .map_err(|e| anyhow!("face {face_idx} failed to parse: {e}"))?;
        let em_px = seg.size.0 * DEVICE_SCALE;
        for g in &seg.glyphs {
            let device = to_device(rt, &g.offset);
            let origin = KPoint::new(device.x, device.y);
            if let Some(path) = build_glyph_bezpath(&face, g.glyph_id as u16, em_px, origin) {
                // NonZero: the reference emitter (`round2-svgref`) fills with
                // fill-rule nonzero, and the recipe states this is the rule
                // to match (§3: "Fill rule nonzero, as the reference emitter
                // uses").
                scene.fill(Fill::NonZero, Affine::IDENTITY, INK, None, &path);
            }
            // `None`: a whitespace glyph with an empty outline. Not an
            // error — `round2-svgref` treats this identically (`empty`, not
            // a failure), and the fixture's own glyph/segment counts already
            // account for it.
        }
    }
    Ok(scene)
}

/// Copies a rendered texture back to host memory as tightly packed RGBA,
/// undoing wgpu's 256-byte row-stride padding. Identical in shape to Round
/// 1's C2 `readback` (duplicated rather than shared, so `src/main.rs` stays
/// byte-identical and untouched by this packet).
fn readback(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Result<Vec<u8>> {
    let unpadded = width * 4;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded = unpadded.div_ceil(align) * align;

    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("c2-round2-readback"),
        size: (padded as u64) * (height as u64),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("c2-round2-copy"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);

    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::PollType::wait_indefinitely())?;
    rx.recv()
        .map_err(|e| anyhow!("readback channel closed: {e}"))?
        .map_err(|e| anyhow!("buffer map failed: {e}"))?;

    let mapped = slice.get_mapped_range();
    let mut out = Vec::with_capacity((unpadded as usize) * (height as usize));
    for row in 0..height as usize {
        let start = row * padded as usize;
        out.extend_from_slice(&mapped[start..start + unpadded as usize]);
    }
    drop(mapped);
    buffer.unmap();
    Ok(out)
}

/// Renders `rt` offscreen at pin 4's 1920x1080, opaque white ground, opaque
/// black ink, returning tightly packed RGBA8 — the exact shape
/// `round2_diff::diff` and `round2-candidatekit`'s loader require.
pub fn render_fixture(
    gpu: &mut Gpu,
    rt: &SpikeResolvedText,
    faces: &[LoadedFace],
) -> Result<Vec<u8>> {
    let scene = build_scene(rt, faces)?;

    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("c2-round2-target"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    gpu.renderer
        .render_to_texture(
            &gpu.device,
            &gpu.queue,
            &scene,
            &view,
            &RenderParams {
                base_color: GROUND,
                width: WIDTH,
                height: HEIGHT,
                antialiasing_method: AA,
            },
        )
        .map_err(|e| anyhow!("vello render_to_texture failed: {e}"))?;

    readback(&gpu.device, &gpu.queue, &texture, WIDTH, HEIGHT)
}
