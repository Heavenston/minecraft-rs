use render_graph_macros::{ input_bundle, output_bundle };
use super::{ graph_resource };

struct NonClone;

graph_resource!(struct First(()));
graph_resource!(struct Second(NonClone));
input_bundle!(struct MyCoolBundleInput(First, ignore ref dyn NonClone, untyped, ref untyped));
output_bundle!(struct MyCoolBundleOutput {
    first: default First,
    dyned: dyn NonClone,
    untyped: untyped,
});
output_bundle!(struct MyCoolBundleOutput2);

struct N;
impl super::GraphNode for N {
    type InputBundle = MyCoolBundleInput;
    type OutputBundle = MyCoolBundleOutput;

    fn run(&mut self, inputs: <Self::InputBundle as crate::ResourceInputBundle>::Values<'_>) -> <Self::OutputBundle as crate::ResourceOutputBundle>::Values {
        let _ = inputs;
        todo!()
    }
}
