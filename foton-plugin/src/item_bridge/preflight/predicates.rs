use foton_registry::data_component_predicate::{
    BundlePredicate, CollectionPredicate, ContainerPredicate, CustomDataPredicate,
    DataComponentPredicateData, WrittenBookPredicate,
};

use super::{Node, Walker};

fn elements<P>(value: &CollectionPredicate<P>) -> impl Iterator<Item = &P> {
    value.contains().into_iter().flatten().chain(
        value
            .counts()
            .into_iter()
            .flatten()
            .map(|entry| entry.test()),
    )
}

impl<'a> Walker<'a> {
    pub(super) fn predicate(&mut self, value: &'a DataComponentPredicateData) {
        if let Some(value) = value.downcast_ref::<CustomDataPredicate>() {
            self.push(Node::Compound(value.value().tag()));
        }
        if let Some(value) = value.downcast_ref::<ContainerPredicate>()
            && let Some(items) = value.items()
        {
            for item in elements(items) {
                self.push(Node::Matchers(item.components()));
            }
        }
        if let Some(value) = value.downcast_ref::<BundlePredicate>()
            && let Some(items) = value.items()
        {
            for item in elements(items) {
                self.push(Node::Matchers(item.components()));
            }
        }
        if let Some(value) = value.downcast_ref::<WrittenBookPredicate>()
            && let Some(pages) = value.pages()
        {
            for page in elements(pages) {
                self.push(Node::Text(page.contents()));
            }
        }
    }
}
