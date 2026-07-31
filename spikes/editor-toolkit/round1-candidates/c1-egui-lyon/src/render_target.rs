//! `ReportPart::TextRendering`, half two: the offscreen render target this
//! candidate draws Round 2 fixtures into for checks 1/2 — `egui_wgpu`
//! device/adapter setup, the MSAA/resolve texture pair, the render pass, and
//! the CPU readback. `glyph_outline.rs` is the other half (outline
//! extraction + tessellation); together they are exactly the F3 cost-schema
//! amendment's definition of this row: "outline extraction, path building,
//! tessellation/rasterization, the offscreen render target."
//!
//! Split out of `bin/c1_round2_text.rs` by the F3 fix: that file used to
//! carry this render pipeline *and* apparatus loading *and* report assembly
//! in one file, which is fine for the packet's own line total but makes the
//! per-part comparison against C2 meaningless — a file can only honestly
//! contribute to one `ReportPart`. This module is `TextRendering`, full
//! stop; `bin/c1_round2_text.rs` now only calls into it.

use anyhow::{anyhow, Context, Result};
use egui::epaint::{ClippedPrimitive, Primitive};
use egui::{Pos2, Rect, TextureId};
use egui_wgpu::wgpu;
use egui_wgpu::{Renderer, RendererOptions, ScreenDescriptor};
use lyon_tessellation::VertexBuffers;

use crate::glyph_outline::{glyph_outline_to_lyon_path, mesh_from_buffers, tessellate_into};
use round2_textkit::types::SpikeResolvedText;

/// Pin 4's offscreen target (restated as a literal, the discipline every
/// loader/emitter in this workspace uses).
pub const WIDTH: u32 = 1920;
pub const HEIGHT: u32 = 1080;
/// Matches Round 1's own C1 configuration (`main.rs`'s `MSAA`) — hardware
/// MSAA render-target attachment, GPU-resolved.
pub const MSAA: u32 = 8;
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const GROUND: wgpu::Color = wgpu::Color {
    r: 1.0,
    g: 1.0,
    b: 1.0,
    a: 1.0,
};

pub struct GpuCtx {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: Renderer,
    white_tex: TextureId,
    pub adapter_name: String,
    pub adapter_device_type: String,
}

pub fn build_gpu() -> Result<GpuCtx> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::VULKAN));
    if adapters.is_empty() {
        return Err(anyhow!(
            "NOT RUN: no Vulkan adapter enumerated — environment absence, not a candidate failure"
        ));
    }
    // Prefer the integrated adapter (pin 4/round 4's deciding figure comes
    // from the integrated adapter) when present; else take whatever
    // enumerated first. Checks 1/2/4 are pixel/geometry correctness checks,
    // not timed figures, so the choice is a reporting detail, not a
    // methodological one — recorded in the printed report either way.
    let adapter = adapters
        .iter()
        .find(|a| a.get_info().device_type == wgpu::DeviceType::IntegratedGpu)
        .unwrap_or(&adapters[0]);
    let info = adapter.get_info();

    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("c1-round2-text"),
        required_features: wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::default(),
        trace: wgpu::Trace::Off,
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
    }))
    .context("wgpu device request failed")?;

    let mut renderer = Renderer::new(
        &device,
        FORMAT,
        RendererOptions {
            msaa_samples: MSAA,
            depth_stencil_format: None,
            ..Default::default()
        },
    );

    // A 1x1 opaque-white texture registered with the renderer — an
    // unregistered `TextureId` is silently skipped by egui's own draw loop
    // (see Round 1's `main.rs` doc comment on `tessellate`), which would
    // read as "every ink sample is background" rather than a build error.
    let white = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("c1-round2-white-1x1"),
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
    let white_tex =
        renderer.register_native_texture(&device, &white_view, wgpu::FilterMode::Nearest);

    Ok(GpuCtx {
        device,
        queue,
        renderer,
        white_tex,
        adapter_name: info.name.clone(),
        adapter_device_type: format!("{:?}", info.device_type),
    })
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
        label: Some("c1-round2-readback"),
        size: (padded as u64) * (height as u64),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("c1-round2-copy"),
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

/// Everything measured while drawing one fixture: the candidate raster, and
/// the check-2 evidence (which segments, if any, resolved to `face: None`
/// and therefore drew nothing).
pub struct FixtureDraw {
    pub rgba: Vec<u8>,
    pub unresolved_segments: Vec<String>,
}

/// Builds the whole fixture's ink as one tessellated mesh directly from
/// `rt`'s own segments/glyphs — **never** from any egui text-layout call,
/// font-fallback API, or `rustybuzz`. A segment with `face: None` (F-C's
/// uncovered Arabic letter) is skipped by construction: its own `glyphs` is
/// already empty (W3-F3 / the resolved-text invariants), so there is
/// nothing to draw and nothing to substitute — recorded in
/// `unresolved_segments` so the report can name it explicitly rather than
/// looking identical to a candidate that silently dropped it.
pub fn draw_fixture(
    gpu: &mut GpuCtx,
    rt: &SpikeResolvedText,
    ttf_faces: &[ttf_parser::Face],
) -> Result<FixtureDraw> {
    let mut buffers: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
    let mut unresolved_segments = Vec::new();

    for seg in &rt.segments {
        let Some(face_idx) = seg.face else {
            assert!(
                seg.glyphs.is_empty(),
                "an unresolved segment (face: None) must carry no glyphs — this candidate never \
                 substitutes a fallback glyph for one"
            );
            let text = rt
                .text
                .get(seg.source.start as usize..seg.source.end as usize)
                .unwrap_or("<non-UTF8-boundary>");
            unresolved_segments.push(format!(
                "source {}..{} ({text:?}): face resolved to None (no declared face covers this \
                 span) — {} glyphs drawn, no substitution",
                seg.source.start,
                seg.source.end,
                seg.glyphs.len()
            ));
            continue;
        };
        let face = ttf_faces.get(face_idx as usize).ok_or_else(|| {
            anyhow!(
                "segment declares face {face_idx}, but only {} faces were loaded",
                ttf_faces.len()
            )
        })?;
        let em_px = seg.size.0 * round2_textkit::DEVICE_SCALE;
        for g in &seg.glyphs {
            let device = round2_textkit::hittest::to_device(rt, &g.offset);
            if let Some(path) =
                glyph_outline_to_lyon_path(face, g.glyph_id, (device.x, device.y), em_px)
            {
                tessellate_into(&path, &mut buffers)
                    .map_err(|e| anyhow!("tessellation failed: {e}"))?;
            }
            // `None`: a whitespace glyph with no outline — draws nothing,
            // exactly as the reference emitter's own `empty` list records.
        }
    }

    let mesh = mesh_from_buffers(&buffers, gpu.white_tex);

    let msaa_tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("c1-round2-msaa"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: MSAA,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let resolve_tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("c1-round2-resolve"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let msaa_view = msaa_tex.create_view(&wgpu::TextureViewDescriptor::default());
    let resolve_view = resolve_tex.create_view(&wgpu::TextureViewDescriptor::default());

    let jobs = vec![ClippedPrimitive {
        clip_rect: Rect::from_min_size(Pos2::ZERO, egui::vec2(WIDTH as f32, HEIGHT as f32)),
        primitive: Primitive::Mesh(mesh),
    }];
    let screen = ScreenDescriptor {
        size_in_pixels: [WIDTH, HEIGHT],
        pixels_per_point: 1.0,
    };

    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("c1-round2-encode"),
        });
    let extra = gpu
        .renderer
        .update_buffers(&gpu.device, &gpu.queue, &mut encoder, &jobs, &screen);
    {
        let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("c1-round2-pass"),
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
        let mut pass = pass.forget_lifetime();
        gpu.renderer.render(&mut pass, &jobs, &screen);
    }
    gpu.queue
        .submit(extra.into_iter().chain([encoder.finish()]));

    let rgba = readback(&gpu.device, &gpu.queue, &resolve_tex, WIDTH, HEIGHT)?;
    Ok(FixtureDraw {
        rgba,
        unresolved_segments,
    })
}
