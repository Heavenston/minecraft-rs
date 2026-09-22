use crate::{ Label, bundles::{ ResourceInputBundle, ResourceOutputBundle } };

pub trait GraphNode: 'static {
    type InputBundle: ResourceInputBundle;
    type OutputBundle: ResourceOutputBundle;

    #[inline]
    fn label(&self) -> Label<'_> {
        Label::TypeName(std::any::type_name::<Self>())
    }

    fn run(&mut self, inputs: <Self::InputBundle as ResourceInputBundle>::Values<'_>) -> <Self::OutputBundle as ResourceOutputBundle>::Values;
}

pub trait FailibleGraphNode: 'static {
    type InputBundle: ResourceInputBundle;
    type OutputBundle: ResourceOutputBundle;

    #[inline]
    fn label(&self) -> Label<'_> {
        Label::TypeName(std::any::type_name::<Self>())
    }

    fn run(&mut self, inputs: <Self::InputBundle as ResourceInputBundle>::Values<'_>) -> anyhow::Result<<Self::OutputBundle as ResourceOutputBundle>::Values>;
}

impl<T> FailibleGraphNode for T
    where T: GraphNode
{
    type InputBundle = <T as GraphNode>::InputBundle;
    type OutputBundle = <T as GraphNode>::OutputBundle;

    fn label(&self) -> Label<'_> {
        <T as GraphNode>::label(self)
    }

    fn run(&mut self, inputs: <Self::InputBundle as ResourceInputBundle>::Values<'_>) -> anyhow::Result<<Self::OutputBundle as ResourceOutputBundle>::Values> {
        Ok(<T as GraphNode>::run(self, inputs))
    }
}
