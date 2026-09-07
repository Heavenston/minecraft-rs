#![recursion_limit = "256"]
#![feature(macro_metavar_expr_concat)]
#![feature(macro_metavar_expr)]

#[macro_export]
macro_rules! node {
    (in $graph:expr; ) => {};
    (in $graph:expr; $name: ident($($arg:tt: $(ref $marker:vis)?$input:ty),*$(,)?) -> ($($output:tt)*) $body: block;$($rest:tt)*) => {
        #[expect(non_snake_case, reason = "macro-generated")]
        mod ${ concat($name, _module) } {
            #[allow(unused, reason = "macro-generated")]
            #[expect(clippy::allow_attributes, reason = "")]
            use super::*;

            render_graph::input_bundle!(pub struct Input($($(ref${ ignore($marker) })?$input),*));
            render_graph::output_bundle!(pub struct Output($($output)*));
            pub struct $name;
            impl render_graph::GraphNode for $name {
                type InputBundle = Input;
                type OutputBundle = Output;
                fn run(&mut self, InputValue($($arg),*): InputValue) -> OutputValue {
                    $body
                }
            }
        }
        use ${ concat($name, _module) }::$name;
        $graph.push_node($name);
        node!(in $graph; $($rest)*)
    };
}

pub mod renderer;
mod winit_loop_handler;
mod inputs_state;

pub use renderer::Renderer;
pub use inputs_state::*;

use anyhow::Result;

pub struct Ctx<'a> {
    pub inputs_state: &'a mut InputsState,
    pub renderer: &'a mut Renderer,
}

pub trait App: 'static {
    fn resume(&mut self, renderer: &mut Renderer) -> Result<()> {
        let _ = renderer;
        Ok(())
    }

    fn update(&mut self, ctx: Ctx<'_>) -> Result<()> {
        let _ = ctx;
        Ok(())
    }
}

pub fn start<A: App>(app: A) -> anyhow::Result<()> {
    let event_loop = winit::event_loop::EventLoop::with_user_event().build()?;
    let mut engine = winit_loop_handler::WinitLoopHandler::new(app);
    event_loop.run_app(&mut engine)?;
    Ok(())
}
