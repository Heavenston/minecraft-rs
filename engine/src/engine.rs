use std::sync::Arc;

use anyhow::Result;
use winit::{application::ApplicationHandler, event::{KeyEvent, WindowEvent}, event_loop::{ActiveEventLoop, OwnedDisplayHandle}, keyboard::{KeyCode, PhysicalKey}, window::Window};

use crate::{InputsState, RenderWorld, Renderer};

pub struct State {
    renderer: Renderer,
    render_world: RenderWorld,
}

impl State {
    async fn new(display_handle: OwnedDisplayHandle, window: Arc<Window>) -> Result<Self> {
        let renderer = Renderer::new(display_handle, window).await?;
        let render_world = renderer.create_world();
        Ok(Self {
            renderer,
            render_world,
        })
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.renderer.resize(width, height);
        if width != 0 && height != 0 {
            self.render_world.set_render_target_size((width, height));
        }
    }

    fn render(&mut self) -> Result<()> {
        self.renderer.render(&mut self.render_world)
    }
}

pub struct Ctx<'a> {
    pub inputs_state: &'a mut InputsState,
    pub render_world: &'a mut RenderWorld,
}

pub trait App: 'static {
    fn resume(&mut self, render_world: &mut RenderWorld) -> Result<()> {
        let _ = render_world;
        Ok(())
    }

    fn update(&mut self, ctx: Ctx<'_>) -> Result<()> {
        let _ = ctx;
        Ok(())
    }
}

pub struct Engine<A> {
    #[cfg(target_arch = "wasm32")]
    proxy: Option<winit::event_loop::EventLoopProxy<State>>,
    state: Option<State>,
    inputs_state: InputsState,
    app: A,
}

impl<A: App> Engine<A> {
    pub fn new(app: A, #[cfg(target_arch = "wasm32")] event_loop: &EventLoop<State>) -> Self {
        #[cfg(target_arch = "wasm32")]
        let proxy = Some(event_loop.create_proxy());
        Self {
            state: None,
            #[cfg(target_arch = "wasm32")]
            proxy,
            inputs_state: InputsState::new(),
            app,
        }
    }
}

impl<A: App> ApplicationHandler<State> for Engine<A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        #[allow(unused_mut)]
        let mut window_attributes = Window::default_attributes();

        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            use winit::platform::web::WindowAttributesExtWebSys;
            
            const CANVAS_ID: &str = "canvas";

            let window = wgpu::web_sys::window().unwrap_throw();
            let document = window.document().unwrap_throw();
            let canvas = document.get_element_by_id(CANVAS_ID).unwrap_throw();
            let html_canvas_element = canvas.unchecked_into();
            window_attributes = window_attributes.with_canvas(Some(html_canvas_element));
        }

        let window = Arc::new(event_loop.create_window(window_attributes).unwrap());

        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut state = pollster::block_on(State::new(event_loop.owned_display_handle(), window)).unwrap();
            self.app.resume(&mut state.render_world).unwrap();
            self.state = Some(state);
        }

        #[cfg(target_arch = "wasm32")]
        {
            // Run the future asynchronously and use the
            // proxy to send the results to the event loop
            if let Some(proxy) = self.proxy.take() {
                wasm_bindgen_futures::spawn_local(async move {
                    assert!(proxy
                        .send_event(
                            State::new(window)
                                .await
                                .expect("Unable to create canvas!!!")
                        )
                        .is_ok())
                });
            }
        }
    }

    #[allow(unused_mut)]
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, mut event: State) {
        // This is where proxy.send_event() ends up
        #[cfg(target_arch = "wasm32")]
        {
            event.window.request_redraw();
            event.resize(
                event.window.inner_size().width,
                event.window.inner_size().height,
            );
        }
        self.app.resume(&mut event.render_world).unwrap();
        self.state = Some(event);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let state = match &mut self.state {
            Some(canvas) => canvas,
            None => return,
        };

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => state.resize(size.width, size.height),
            WindowEvent::RedrawRequested => {
                match self.app.update(Ctx {
                    inputs_state: &mut self.inputs_state,
                    render_world: &mut state.render_world,
                }) {
                    Ok(_) => {}
                    Err(e) => {
                        // Log the error and exit gracefully
                        tracing::error!("{e}");
                        event_loop.exit();
                    }
                }
                self.inputs_state.clear_just_pressed_keys();
                match state.render() {
                    Ok(_) => {}
                    Err(e) => {
                        // Log the error and exit gracefully
                        tracing::error!("{e}");
                        event_loop.exit();
                    }
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(code),
                        state: key_state,
                        repeat: false,
                        ..
                    },
                ..
            } => match (code, key_state.is_pressed()) {
                (KeyCode::Escape, true) => event_loop.exit(),
                _ => {
                    self.inputs_state.register_key_event(code, key_state.is_pressed());
                }
            },
            _ => {}
        }
    }
}
