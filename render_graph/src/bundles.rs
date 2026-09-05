use crate::{ ResourceGatherer, ResourceInfoProvider, ResourceStorer, UntypedResourceHandle };

pub trait ResourceInputBundle {
    type Values<'a>;

    fn list_consumes(&self, info_provider: &mut impl ResourceInfoProvider) -> impl IntoIterator<Item = UntypedResourceHandle>;
    fn list_borrows(&self, info_provider: &mut impl ResourceInfoProvider) -> impl IntoIterator<Item = UntypedResourceHandle>;
    fn gather<'a>(&self, gatherer: &'a mut impl ResourceGatherer) -> Self::Values<'a>;
}

pub trait ResourceOutputBundle {
    type Values;

    fn list_resources(&self, info_provider: &mut impl ResourceInfoProvider) -> impl IntoIterator<Item = UntypedResourceHandle>;
    fn store(&self, values: Self::Values, storer: &mut impl ResourceStorer);
}
