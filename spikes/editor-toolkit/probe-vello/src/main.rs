//! Round 0 accessibility-route probe, candidate C2 (vello + winit).
//!
//! vello ships no widget toolkit and no accessibility integration of its
//! own, so unlike C1 this is a MANUAL accesskit_winit wiring: the
//! accessibility tree (one window node containing one button node with a
//! distinctive name) is built by hand and pushed through
//! `accesskit_winit::Adapter`, driven from the same winit
//! `ApplicationHandler` that owns the vello `RenderContext`/`Renderer`/
//! `Scene`. This mirrors AccessKit's own upstream `winit` adapter example
//! (`adapters/winit/examples/simple.rs` at AccessKit/accesskit@main),
//! adapted to also drive a real vello render pass so the probe is a
//! genuine instance of "vello behind a winit shell", not accessibility
//! wiring alone.

use anyhow::Result;
use std::sync::Arc;

use accesskit::{
    Action, Node as AccessNode, NodeId as AccessNodeId, Role, Tree, TreeId, TreeUpdate,
};
use accesskit_winit::{Adapter, Event as AccessKitEvent, WindowEvent as AccessKitWindowEvent};

use vello::kurbo::{Affine, RoundedRect};
use vello::peniko::Color;
use vello::util::{RenderContext, RenderSurface};
use vello::wgpu::{self, CurrentSurfaceTexture};
use vello::{AaConfig, Renderer, RendererOptions, Scene};

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

const WINDOW_TITLE: &str = "EpiphanyProbeVello";
const WINDOW_NODE_ID: AccessNodeId = AccessNodeId(0);
const BUTTON_NODE_ID: AccessNodeId = AccessNodeId(1);
const BUTTON_NAME: &str = "EpiphanyProbeButton";

fn build_button() -> AccessNode {
    let mut node = AccessNode::new(Role::Button);
    node.set_label(BUTTON_NAME);
    node.add_action(Action::Focus);
    node
}

fn build_root() -> AccessNode {
    let mut node = AccessNode::new(Role::Window);
    node.set_children(vec![BUTTON_NODE_ID]);
    node.set_label(WINDOW_TITLE);
    node
}

fn build_initial_tree() -> TreeUpdate {
    TreeUpdate {
        nodes: vec![
            (WINDOW_NODE_ID, build_root()),
            (BUTTON_NODE_ID, build_button()),
        ],
        tree: Some(Tree::new(WINDOW_NODE_ID)),
        tree_id: TreeId::ROOT,
        focus: BUTTON_NODE_ID,
    }
}

enum RenderState {
    Active {
        surface: Box<RenderSurface<'static>>,
        valid_surface: bool,
        window: Arc<Window>,
        adapter: Adapter,
    },
    Suspended(Option<Arc<Window>>),
}

struct ProbeApp {
    event_loop_proxy: EventLoopProxy<AccessKitEvent>,
    context: RenderContext,
    renderers: Vec<Option<Renderer>>,
    state: RenderState,
    scene: Scene,
}

impl ProbeApp {
    fn new(event_loop_proxy: EventLoopProxy<AccessKitEvent>) -> Self {
        Self {
            event_loop_proxy,
            context: RenderContext::new(),
            renderers: vec![],
            state: RenderState::Suspended(None),
            scene: Scene::new(),
        }
    }
}

impl ApplicationHandler<AccessKitEvent> for ProbeApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let RenderState::Suspended(cached_window) = &mut self.state else {
            return;
        };

        let window = cached_window.take().unwrap_or_else(|| {
            let attr = Window::default_attributes()
                .with_inner_size(LogicalSize::new(320, 180))
                .with_title(WINDOW_TITLE);
            Arc::new(event_loop.create_window(attr).unwrap())
        });

        // Manual accesskit_winit wiring: one adapter per window, driven by
        // the same event loop proxy that feeds this ApplicationHandler.
        let adapter =
            Adapter::with_event_loop_proxy(event_loop, &window, self.event_loop_proxy.clone());

        let size = window.inner_size();
        let surface_future = self.context.create_surface(
            window.clone(),
            size.width,
            size.height,
            wgpu::PresentMode::AutoVsync,
        );
        let surface = pollster::block_on(surface_future).expect("Error creating vello surface");

        self.renderers
            .resize_with(self.context.devices.len(), || None);
        self.renderers[surface.dev_id].get_or_insert_with(|| {
            Renderer::new(
                &self.context.devices[surface.dev_id].device,
                RendererOptions::default(),
            )
            .expect("Couldn't create vello renderer")
        });

        self.state = RenderState::Active {
            surface: Box::new(surface),
            valid_surface: true,
            window,
            adapter,
        };
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        if let RenderState::Active { window, .. } = &self.state {
            self.state = RenderState::Suspended(Some(window.clone()));
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let (surface, valid_surface, window, adapter) = match &mut self.state {
            RenderState::Active {
                surface,
                valid_surface,
                window,
                adapter,
            } if window.id() == window_id => (surface, valid_surface, window, adapter),
            _ => return,
        };

        adapter.process_event(window, &event);

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if size.width != 0 && size.height != 0 {
                    self.context
                        .resize_surface(surface, size.width, size.height);
                    *valid_surface = true;
                } else {
                    *valid_surface = false;
                }
            }
            WindowEvent::RedrawRequested => {
                if !*valid_surface {
                    return;
                }
                self.scene.reset();
                let rect = RoundedRect::new(20.0, 20.0, 200.0, 64.0, 6.0);
                self.scene.fill(
                    vello::peniko::Fill::NonZero,
                    Affine::IDENTITY,
                    Color::new([0.7, 0.85, 1.0, 1.0]),
                    None,
                    &rect,
                );

                let width = surface.config.width;
                let height = surface.config.height;
                let device_handle = &self.context.devices[surface.dev_id];

                self.renderers[surface.dev_id]
                    .as_mut()
                    .unwrap()
                    .render_to_texture(
                        &device_handle.device,
                        &device_handle.queue,
                        &self.scene,
                        &surface.target_view,
                        &vello::RenderParams {
                            base_color: Color::new([0.05, 0.05, 0.08, 1.0]),
                            width,
                            height,
                            antialiasing_method: AaConfig::Msaa16,
                        },
                    )
                    .expect("failed to render");

                let surface_texture = match surface.surface.get_current_texture() {
                    CurrentSurfaceTexture::Success(t) => t,
                    CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Suboptimal(_) => {
                        self.context.configure_surface(surface);
                        window.request_redraw();
                        return;
                    }
                    CurrentSurfaceTexture::Occluded | CurrentSurfaceTexture::Timeout => {
                        window.request_redraw();
                        return;
                    }
                    CurrentSurfaceTexture::Lost => panic!("Surface was lost"),
                    CurrentSurfaceTexture::Validation => panic!("Validation error getting surface"),
                };

                let mut encoder =
                    device_handle
                        .device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("Surface Blit"),
                        });
                surface.blitter.copy(
                    &device_handle.device,
                    &mut encoder,
                    &surface.target_view,
                    &surface_texture
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default()),
                );
                device_handle.queue.submit([encoder.finish()]);
                surface_texture.present();
                device_handle.device.poll(wgpu::PollType::Poll).unwrap();
            }
            _ => {}
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, user_event: AccessKitEvent) {
        let RenderState::Active { adapter, .. } = &mut self.state else {
            return;
        };
        if let AccessKitWindowEvent::InitialTreeRequested = user_event.window_event {
            adapter.update_if_active(build_initial_tree);
        }
    }
}

fn main() -> Result<()> {
    let event_loop = EventLoop::<AccessKitEvent>::with_user_event().build()?;
    let mut app = ProbeApp::new(event_loop.create_proxy());
    event_loop.run_app(&mut app)?;
    Ok(())
}
