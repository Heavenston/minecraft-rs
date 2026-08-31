pub use harness::{ Renderer };

pub trait App: 'static {
    
}

struct HarnessApp<A: App> {
    #[expect(unused)]
    app: A,
}

impl<A: App> harness::App for HarnessApp<A> {
    fn resume(&mut self, renderer: &mut Renderer) -> anyhow::Result<()> {
        let _ = renderer;
        Ok(())
    }

    fn update(&mut self, ctx: harness::Ctx<'_>) -> anyhow::Result<()> {
        let _ = ctx;
        Ok(())
    }
}

pub fn start<A: App>(app: A) -> anyhow::Result<()> {
    harness::start(HarnessApp {
        app,
    })?;
    Ok(())
}
