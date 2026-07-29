//! Round 1 candidate **C2 — vello + kurbo**.
//!
//! Renders each Round-1 glyph's typed `PathCommand` outline through vello's
//! own scene/renderer pipeline to an offscreen texture, then hands the RGBA
//! readback to the candidate-neutral harness for classification against the
//! **frozen** `round1-oracle/oracle.json`. This binary never reads or writes
//! that oracle directly — the harness owns it, read-only.
//!
//! Windowless by construction: vello's `Renderer::render_to_texture` needs no
//! surface, so no `winit` appears here even though C2's Round-0 accessibility
//! route did use it.
//!
//! **The whole outline goes into one `BezPath` and one `Scene::fill` call.**
//! That is the point of criterion 1: a compound path with several contours,
//! filled as one shape, so bounded counters must survive as holes and disjoint
//! components must all be painted. Filling each subpath separately would paint
//! counters solid and quietly pass the test it exists to fail.

use anyhow::{anyhow, Result};
use epiphany_layout_ir::PathCommand;
use round1_harness as harness;
use vello::kurbo::{Affine, BezPath, Point as KPoint};
use vello::peniko::{color::palette, Color, Fill};
use vello::wgpu;
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};

/// Opaque black ink on an opaque white ground — the harness classifies by
/// luminance against this contrast, and the oracle's >= 8 device-px clearance
/// guarantees every sample lands fully saturated.
const INK: Color = palette::css::BLACK;
const GROUND: Color = palette::css::WHITE;

/// Rec. 601 luma threshold used by the harness. Stated, not implied, so a
/// FAIL's root cause can never be "which threshold did you mean".
const LUMA_THRESHOLD: u8 = 128;

/// **Nominally 8x — the highest sample count BOTH candidates name**, per pin
/// 4's "identical MSAA sample count... the highest all survivors support".
/// vello's `AaConfig` offers only Area / Msaa8 / Msaa16, so 8x is the ceiling
/// on this side, and C1 now matches that integer rather than the 4x it ran
/// earlier.
///
/// **The integers match; the mechanisms do not, and this is not a footnote.**
/// C1's 8x is hardware multisampling — a `sample_count: 8` colour attachment
/// resolved by the GPU, which is precisely why C1 has to request
/// `TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES` (8x on `Rgba8Unorm` is off the
/// WebGPU baseline). vello's `Msaa8` is its own compute-shader antialiasing,
/// writing into the `sample_count: 1` storage texture created below; there is
/// no multisample attachment anywhere on this side. So pin 4's identical-
/// configuration requirement is met **as stated** — same declared sample count
/// — but equal integers do not imply equal work, equal memory traffic, or
/// equal cost. Round 1 is a capability round and is indifferent to that; Round
/// 4 is not, because AA lands directly in the deciding latency numbers. The
/// report carries the mechanism next to the number so no later round can read
/// "8 == 8" as "same thing measured".
const AA: AaConfig = AaConfig::Msaa8;

/// Printed beside `msaa_samples` in the run report, so the record itself
/// states the mechanism rather than leaving the bare integer to imply parity.
const AA_MECHANISM: &str = "vello compute AA into a sample_count:1 storage texture";

/// vello's `render_to_texture` requires `Rgba8Unorm` + `STORAGE_BINDING`, so
/// both candidates are pinned to this same non-sRGB format for a fair
/// comparison. Immaterial to fill correctness (0 and 255 map to themselves
/// under any transfer function) but a literal deviation from pin 4's "sRGB
/// target format", and reported as one.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Builds one `kurbo::BezPath` carrying **every** contour of the outline.
///
/// Staff-space is y-up and device space is y-down, so the y axis is flipped
/// here rather than in a transform, keeping the path in final device
/// coordinates and matching the oracle's own
/// `device = (staff.x * scale + tx, ty - staff.y * scale)` rule exactly.
fn build_path(outline: &[PathCommand], t: &harness::Transform) -> BezPath {
    let map = |p: epiphany_layout_ir::Point| -> KPoint {
        KPoint::new(p.x.0 as f64 * t.scale + t.tx, t.ty - p.y.0 as f64 * t.scale)
    };
    let mut path = BezPath::new();
    let mut open = false;
    for cmd in outline {
        match cmd {
            PathCommand::MoveTo(p) => {
                if open {
                    path.close_path();
                }
                path.move_to(map(*p));
                open = true;
            }
            PathCommand::LineTo(p) => path.line_to(map(*p)),
            PathCommand::CurveTo {
                control1,
                control2,
                to,
            } => path.curve_to(map(*control1), map(*control2), map(*to)),
            PathCommand::Close => {
                path.close_path();
                open = false;
            }
        }
    }
    if open {
        path.close_path();
    }
    path
}

/// Copies a rendered texture back to host memory as tightly packed RGBA,
/// undoing wgpu's 256-byte row-stride padding.
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
        label: Some("c2-readback"),
        size: (padded as u64) * (height as u64),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("c2-copy"),
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

fn adapter_info(a: &wgpu::Adapter) -> harness::AdapterInfo {
    let info = a.get_info();
    harness::AdapterInfo {
        name: info.name.clone(),
        backend: format!("{:?}", info.backend),
        device_type: format!("{:?}", info.device_type),
        vendor_id: info.vendor,
        device_id: info.device,
    }
}

fn run_on(adapter: &wgpu::Adapter, oracle: &harness::OracleFile) -> Result<harness::RunReport> {
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("c2-vello"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::default(),
        trace: wgpu::Trace::Off,
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
    }))?;

    let mut renderer = Renderer::new(
        &device,
        RendererOptions {
            use_cpu: false,
            antialiasing_support: AaSupport::all(),
            num_init_threads: None,
            pipeline_cache: None,
        },
    )
    .map_err(|e| anyhow!("vello Renderer::new failed: {e}"))?;

    let mut glyphs = Vec::new();
    for g in &oracle.glyphs {
        let width = g.transform.target_width as u32;
        let height = g.transform.target_height as u32;

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("c2-target"),
            size: wgpu::Extent3d {
                width,
                height,
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

        let outline = harness::outline_for(&g.name);
        let path = build_path(&outline, &g.transform);

        let mut scene = Scene::new();
        // NonZero: Bravura's contours are correctly oppositely wound, so
        // even-odd and nonzero agree on every bundled counter — the oracle
        // records that measurement. The fill *rule* is not what is under test;
        // preserving contours and counters is.
        scene.fill(Fill::NonZero, Affine::IDENTITY, INK, None, &path);

        renderer
            .render_to_texture(
                &device,
                &queue,
                &scene,
                &view,
                &RenderParams {
                    base_color: GROUND,
                    width,
                    height,
                    antialiasing_method: AA,
                },
            )
            .map_err(|e| anyhow!("{}: vello render_to_texture failed: {e}", g.name))?;

        let rgba = readback(&device, &queue, &texture, width, height)?;
        glyphs.push(
            harness::evaluate_glyph(g, width, height, &rgba, LUMA_THRESHOLD)
                .map_err(|e| anyhow!("{e}"))?,
        );
    }

    Ok(harness::RunReport {
        candidate: "C2 vello 0.9 + kurbo".to_string(),
        adapter: adapter_info(adapter),
        msaa_samples: 8,
        aa_mechanism: AA_MECHANISM.to_string(),
        target_format: format!("{FORMAT:?}"),
        luminance_threshold: LUMA_THRESHOLD,
        fill_rule: "NonZero".to_string(),
        glyphs,
        notes: vec![
            "AA is nominally 8x on both candidates — the highest sample count both name, so pin \
             4's identical-configuration requirement is met as stated. The mechanisms are NOT the \
             same: C1 uses a hardware multisample render-target attachment (hence its \
             TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES request), C2 uses vello's compute-shader AA \
             into a sample_count:1 storage texture. Matching integers do not imply matching work \
             or matching cost; this is immaterial to Round 1's capability verdict and material to \
             Round 4's timings."
                .to_string(),
            "Format deviation: Rgba8Unorm, not sRGB — vello's render_to_texture requires \
             Rgba8Unorm + STORAGE_BINDING. Both candidates pinned to it for fairness; immaterial \
             to fill correctness since only pure black/white are drawn."
                .to_string(),
            "Whole outline filled as ONE compound BezPath in one Scene::fill call.".to_string(),
        ],
    })
}

fn main() -> Result<()> {
    let oracle = harness::load_oracle();
    // Semantic validation before anything renders: an oracle that deserializes
    // but no longer means what Round 1 requires would be tested faithfully and
    // pass (harness `OracleFile::validate`).
    oracle
        .validate()
        .map_err(|e| anyhow!("oracle failed validation: {e}"))?;

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });

    let adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::VULKAN));

    // Pin 4 requires BOTH adapter classes, and the integrated figure is the one
    // that decides. Reporting overall PASS after testing whichever adapter
    // happened to enumerate would silently narrow the claim, so a missing class
    // is NOT RUN (an environment absence) and never a pass.
    let has_discrete = adapters
        .iter()
        .any(|a| a.get_info().device_type == wgpu::DeviceType::DiscreteGpu);
    let has_integrated = adapters
        .iter()
        .any(|a| a.get_info().device_type == wgpu::DeviceType::IntegratedGpu);
    if !has_discrete || !has_integrated {
        let found: Vec<String> = adapters
            .iter()
            .map(|a| {
                let i = a.get_info();
                format!("{} ({:?})", i.name, i.device_type)
            })
            .collect();
        return Err(anyhow!(
            "NOT RUN: pin 4 requires one discrete and one integrated Vulkan adapter; found \
             {found:?}. This is an environment absence, not a candidate failure — re-run where \
             both are present."
        ));
    }

    let mut all_pass = true;
    for adapter in &adapters {
        let report = run_on(adapter, &oracle)?;
        print!("{}", report.table());
        let total: usize = report.glyphs.iter().map(|g| g.points.len()).sum();
        let passed: usize = report
            .glyphs
            .iter()
            .map(|g| g.points.iter().filter(|p| p.pass).count())
            .sum();
        println!(
            "RESULT {} :: {passed}/{total} points PASS\n",
            report.adapter.name
        );
        all_pass &= report.all_pass();
    }

    if all_pass {
        println!("C2 vello: PASS on all {} adapter(s)", adapters.len());
        Ok(())
    } else {
        Err(anyhow!("C2 vello: FAIL — see the per-point table above"))
    }
}
