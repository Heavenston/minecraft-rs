use std::sync::Arc;

use winit::{application::ApplicationHandler, event::{KeyEvent, WindowEvent}, event_loop::ActiveEventLoop, keyboard::{KeyCode, PhysicalKey}, window::Window};

use crate::{App, Ctx, InputsState, Renderer};

pub struct WinitLoopHandler<A> {
    #[cfg(target_arch = "wasm32")]
    proxy: Option<winit::event_loop::EventLoopProxy<State>>,
    renderer: Option<Renderer>,
    inputs_state: InputsState,
    app: A,
}

impl<A: App> WinitLoopHandler<A> {
    #[expect(clippy::single_call_fn, reason = "only for starting engine")]
    pub fn new(app: A, #[cfg(target_arch = "wasm32")] event_loop: &EventLoop<State>) -> Self {
        #[cfg(target_arch = "wasm32")]
        let proxy = Some(event_loop.create_proxy());
        Self {
            #[cfg(target_arch = "wasm32")]
            proxy,
            renderer: None,
            inputs_state: InputsState::new(),
            app,
        }
    }
}

impl<A: App> ApplicationHandler<Renderer> for WinitLoopHandler<A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        #[expect(clippy::allow_attributes, reason = "only for wasm32")]
        #[allow(unused_mut, reason = "Mut required for wasm32")]
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
            let mut renderer = pollster::block_on(Renderer::new(event_loop.owned_display_handle(), window)).unwrap();
            self.app.resume(&mut renderer).unwrap();
            self.renderer = Some(renderer);
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

    #[expect(clippy::allow_attributes, reason = "only for wasm32")]
    #[allow(unused_mut, reason = "Mut required for wasm32")]
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, mut event: Renderer) {
        // This is where proxy.send_event() ends up
        #[cfg(target_arch = "wasm32")]
        {
            event.window.request_redraw();
            event.resize(
                event.window.inner_size().width,
                event.window.inner_size().height,
            );
        }
        self.app.resume(&mut event).unwrap();
        self.renderer = Some(event);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let Some(renderer) = &mut self.renderer
        else { return };

        #[expect(clippy::wildcard_enum_match_arm, reason = "Not catching most variants is expected")]
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => renderer.resize(size.width, size.height),
            WindowEvent::RedrawRequested => {
                match self.app.update(Ctx {
                    inputs_state: &mut self.inputs_state,
                    renderer,
                }) {
                    Ok(()) => {}
                    Err(e) => {
                        // Log the error and exit gracefully
                        tracing::error!("{e}");
                        event_loop.exit();
                    }
                }
                self.inputs_state.clear_just_pressed_keys();
                match renderer.render() {
                    Ok(()) => {}
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
