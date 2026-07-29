//! Round 1 candidate **C1 — egui + lyon**.
//!
//! Tessellates each Round-1 glyph's typed `PathCommand` outline with `lyon`
//! into an `egui::epaint::Mesh`, then draws it through **egui's own paint
//! pipeline** (`egui_wgpu::Renderer`) to an offscreen texture. The RGBA
//! readback goes to the candidate-neutral harness for classification against
//! the **frozen** `round1-oracle/oracle.json`.
//!
//! **Why it must go through `egui_wgpu::Renderer` and not a hand-rolled wgpu
//! pipeline.** Ruling A names the candidate as "lyon-tessellated meshes inside
//! egui". Rendering the lyon mesh through a bare pipeline would test lyon
//! alone and answer a different question — the same failure shape as Round 0's
//! iced side channel, which read back cleanly while proving nothing about its
//! subject. So this binary builds a real `epaint::ClippedPrimitive` and calls
//! `Renderer::update_buffers` + `Renderer::render`.
//!
//! Windowless: egui's renderer needs a `Device`/`Queue` and a target view, not
//! a surface, so no `eframe`/`winit` appears here.
//!
//! **The whole outline is tessellated as ONE compound path.** `lyon`'s
//! `FillTessellator` applies the fill rule across every contour in the path,
//! so bounded counters survive as holes and disjoint components are all
//! painted. Tessellating each subpath separately would fill counters solid and
//! silently pass the test that exists to catch exactly that.

use anyhow::{anyhow, Result};
use egui::epaint::{ClippedPrimitive, Mesh, Primitive, Vertex};
use egui::{Color32, Pos2, Rect, TextureId};
use egui_wgpu::wgpu;
use egui_wgpu::{Renderer, RendererOptions, ScreenDescriptor};
use epiphany_layout_ir::PathCommand;
use lyon_path::math::point as lyon_point;
use lyon_path::Path as LyonPath;
use lyon_tessellation::{
    BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, VertexBuffers,
};
use round1_harness as harness;

const INK: Color32 = Color32::BLACK;
const GROUND: wgpu::Color = wgpu::Color {
    r: 1.0,
    g: 1.0,
    b: 1.0,
    a: 1.0,
};

/// Rec. 601 luma threshold used by the harness — stated, not implied.
const LUMA_THRESHOLD: u8 = 128;

/// **Nominally 8x — the highest sample count BOTH candidates name**, which is
/// what pin 4 asks for ("identical MSAA sample count... 4x, or the highest all
/// survivors support"). An earlier revision ran C1 at 4x against C2's 8x,
/// which is not a common configuration at all: vello's `AaConfig` offers only
/// Area/Msaa8/Msaa16, so 8x is the highest both can name, and both adapters
/// advertise it.
///
/// **This 8x is hardware multisampling and C2's is not.** Here it is a real
/// `sample_count: 8` colour attachment resolved by the GPU — which is exactly
/// why this binary must request `TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES`,
/// since 8x on `Rgba8Unorm` is outside the WebGPU baseline. vello's `Msaa8` is
/// its own compute-shader antialiasing into a single-sample storage texture,
/// with no multisample attachment at all. The declared sample count is
/// identical, as pin 4 requires; the work behind it is not, and Round 4's
/// timings must say so rather than let "8 == 8" stand in for parity.
const MSAA: u32 = 8;

/// Printed beside `msaa_samples` in the run report, so the record itself
/// states the mechanism rather than leaving the bare integer to imply parity.
const AA_MECHANISM: &str = "hardware MSAA render-target attachment, GPU-resolved";

/// `Rgba8Unorm`, not sRGB, so both candidates share one format — vello's
/// `render_to_texture` requires it. A literal deviation from pin 4, immaterial
/// to fill correctness because only pure black and white are drawn.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Builds one `lyon` path carrying **every** contour of the outline, already
/// in device coordinates.
///
/// Staff-space is y-up, device space y-down, so the flip happens here rather
/// than in a transform — keeping this identical to the oracle's own
/// `device = (staff.x * scale + tx, ty - staff.y * scale)` rule.
fn build_path(outline: &[PathCommand], t: &harness::Transform) -> LyonPath {
    let map = |p: epiphany_layout_ir::Point| {
        lyon_point(
            (p.x.0 as f64 * t.scale + t.tx) as f32,
            (t.ty - p.y.0 as f64 * t.scale) as f32,
        )
    };
    let mut builder = LyonPath::builder();
    let mut open = false;
    for cmd in outline {
        match cmd {
            PathCommand::MoveTo(p) => {
                if open {
                    builder.end(true);
                }
                builder.begin(map(*p));
                open = true;
            }
            PathCommand::LineTo(p) => {
                builder.line_to(map(*p));
            }
            PathCommand::CurveTo {
                control1,
                control2,
                to,
            } => {
                builder.cubic_bezier_to(map(*control1), map(*control2), map(*to));
            }
            PathCommand::Close => {
                if open {
                    builder.end(true);
                    open = false;
                }
            }
        }
    }
    if open {
        builder.end(true);
    }
    builder.build()
}

/// Tessellates the compound path into an `epaint::Mesh` bound to `tex`.
///
/// **`tex` must be a texture actually registered with the renderer.** egui's
/// draw loop does `if let Some(..) = self.textures.get(&mesh.texture_id)`
/// (`egui-wgpu-0.35.0/src/renderer.rs:542`) and **silently skips** the
/// primitive when the id is unknown — no error, no warning, just an unpainted
/// mesh. Using `TextureId::default()` (the font atlas) without uploading one
/// therefore renders a blank target that reads as "every ink sample is
/// background", which is a probe defect wearing a candidate failure's costume.
/// This binary registers its own 1x1 opaque-white texture instead, so the
/// vertex colour passes through unmodulated at any UV.
fn tessellate(path: &LyonPath, tex: TextureId) -> Result<Mesh> {
    let mut buffers: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
    let mut tess = FillTessellator::new();
    tess.tessellate_path(
        path,
        // NonZero: Bravura's contours are correctly oppositely wound, so
        // even-odd and nonzero agree on every bundled counter (the oracle
        // records that measurement). The rule is not what is under test —
        // preserving contours and counters is.
        &FillOptions::default().with_fill_rule(FillRule::NonZero),
        &mut BuffersBuilder::new(&mut buffers, |v: FillVertex| {
            let p = v.position();
            [p.x, p.y]
        }),
    )
    .map_err(|e| anyhow!("lyon tessellation failed: {e:?}"))?;

    let mut mesh = Mesh::with_texture(tex);
    mesh.vertices = buffers
        .vertices
        .iter()
        .map(|[x, y]| Vertex {
            pos: Pos2::new(*x, *y),
            uv: Pos2::ZERO,
            color: INK,
        })
        .collect();
    mesh.indices = buffers.indices;
    Ok(mesh)
}

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
        label: Some("c1-readback"),
        size: (padded as u64) * (height as u64),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("c1-copy"),
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
        label: Some("c1-egui-lyon"),
        // 8x MSAA on Rgba8Unorm is not a WebGPU baseline guarantee — the spec
        // guarantees only [1, 4] for this format, and wgpu rejects the
        // pipeline without this feature. Both target adapters report
        // [1, 2, 4, 8] with it enabled, so requesting it is what lets C1 meet
        // pin 4's common-sample-count requirement rather than silently
        // dropping to 4x.
        required_features: wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::default(),
        trace: wgpu::Trace::Off,
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
    }))?;

    let mut renderer = Renderer::new(
        &device,
        FORMAT,
        RendererOptions {
            msaa_samples: MSAA,
            depth_stencil_format: None,
            ..Default::default()
        },
    );

    // A 1x1 opaque-white texture, registered so the mesh's texture id resolves
    // (see `tessellate`'s doc for why an unregistered id silently paints
    // nothing). White modulates the vertex colour by 1.0, so the ink colour is
    // unchanged.
    let white = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("c1-white-1x1"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &white,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &[255u8, 255, 255, 255],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    let white_view = white.create_view(&wgpu::TextureViewDescriptor::default());
    let white_id =
        renderer.register_native_texture(&device, &white_view, wgpu::FilterMode::Nearest);

    let mut glyphs = Vec::new();
    for g in &oracle.glyphs {
        let width = g.transform.target_width as u32;
        let height = g.transform.target_height as u32;

        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        // The MSAA colour attachment, resolved into `resolve` below.
        let msaa_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("c1-msaa"),
            size,
            mip_level_count: 1,
            sample_count: MSAA,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let resolve = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("c1-resolve"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let msaa_view = msaa_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let resolve_view = resolve.create_view(&wgpu::TextureViewDescriptor::default());

        let outline = harness::outline_for(&g.name);
        let path = build_path(&outline, &g.transform);
        let mesh = tessellate(&path, white_id)?;

        let jobs = vec![ClippedPrimitive {
            clip_rect: Rect::from_min_size(Pos2::ZERO, egui::vec2(width as f32, height as f32)),
            primitive: Primitive::Mesh(mesh),
        }];
        let screen = ScreenDescriptor {
            size_in_pixels: [width, height],
            pixels_per_point: 1.0,
        };

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("c1-encode"),
        });
        let extra = renderer.update_buffers(&device, &queue, &mut encoder, &jobs, &screen);

        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("c1-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &msaa_view,
                    resolve_target: Some(&resolve_view),
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(GROUND),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            // `Renderer::render` requires a 'static pass.
            let mut pass = pass.forget_lifetime();
            renderer.render(&mut pass, &jobs, &screen);
        }

        queue.submit(extra.into_iter().chain([encoder.finish()]));

        let rgba = readback(&device, &queue, &resolve, width, height)?;
        glyphs.push(
            harness::evaluate_glyph(g, width, height, &rgba, LUMA_THRESHOLD)
                .map_err(|e| anyhow!("{e}"))?,
        );
    }

    Ok(harness::RunReport {
        candidate: "C1 egui 0.35 + lyon 1.0 (egui_wgpu::Renderer)".to_string(),
        adapter: adapter_info(adapter),
        msaa_samples: MSAA,
        aa_mechanism: AA_MECHANISM.to_string(),
        target_format: format!("{FORMAT:?}"),
        luminance_threshold: LUMA_THRESHOLD,
        fill_rule: "NonZero".to_string(),
        glyphs,
        notes: vec![
            "Rendered through egui's own paint pipeline (egui_wgpu::Renderer::update_buffers + \
             ::render) with an epaint::Mesh, NOT a hand-rolled wgpu pipeline — the stack under \
             test is the real one."
                .to_string(),
            "Whole outline tessellated as ONE compound lyon path in one tessellate_path call."
                .to_string(),
            "Format deviation: Rgba8Unorm, not sRGB, to match C2 (vello requires it).".to_string(),
            "AA is nominally 8x on both candidates — the highest sample count both name, so pin \
             4's identical-configuration requirement is met as stated. The mechanisms are NOT the \
             same: this is a hardware multisample render-target attachment (hence the \
             TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES request), while C2 uses vello's \
             compute-shader AA into a sample_count:1 storage texture. Matching integers do not \
             imply matching work or matching cost; this is immaterial to Round 1's capability \
             verdict and material to Round 4's timings."
                .to_string(),
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
        println!("C1 egui+lyon: PASS on all {} adapter(s)", adapters.len());
        Ok(())
    } else {
        Err(anyhow!(
            "C1 egui+lyon: FAIL — see the per-point table above"
        ))
    }
}
