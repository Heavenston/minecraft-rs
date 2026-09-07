use crate::{ Label, bundles::{ ResourceInputBundle, ResourceOutputBundle } };

pub trait GraphNode: 'static {
    type InputBundle: ResourceInputBundle;
    type OutputBundle: ResourceOutputBundle;

    fn label(&self) -> Label<'_> {
        Label::TypeName(std::any::type_name::<Self>())
    }

    fn run(&mut self, inputs: <Self::InputBundle as ResourceInputBundle>::Values<'_>) -> <Self::OutputBundle as ResourceOutputBundle>::Values;
}
