use text_components::TextComponent;
use text_components::content::{Content, Object, Resolvable};
use text_components::custom::{CustomData, Payload};
use text_components::interactivity::{ClickEvent, Dialog, HoverEvent};

use super::{Node, Walker};

impl<'a> Walker<'a> {
    pub(super) fn text(&mut self, value: &'a TextComponent) {
        for child in &value.children {
            self.push(Node::Text(child));
        }
        match &value.content {
            Content::Translate(message) => {
                for argument in message.args.iter().flat_map(|args| args.iter()) {
                    self.push(Node::Text(argument));
                }
            }
            Content::Object(Object::Atlas { fallback, .. } | Object::Player { fallback, .. }) => {
                if let Some(fallback) = fallback {
                    self.push(Node::Text(fallback));
                }
            }
            Content::Resolvable(
                Resolvable::Entity { separator, .. } | Resolvable::NBT { separator, .. },
            ) => {
                if let Some(separator) = separator {
                    self.push(Node::Text(separator));
                }
            }
            Content::Custom(data) => self.custom_text(data),
            _ => {}
        }
        match &value.interactions.hover {
            Some(HoverEvent::ShowText { value }) => self.push(Node::Text(value)),
            Some(HoverEvent::ShowEntity {
                name: Some(value), ..
            }) => self.push(Node::Text(value)),
            Some(HoverEvent::ShowItem {
                components: Some(value),
                ..
            }) => self.push(Node::Tag(value.as_nbt())),
            _ => {}
        }
        match &value.interactions.click {
            Some(ClickEvent::ShowDialog {
                dialog: Dialog::Inline(value),
            }) => self.push(Node::Tag(value.as_nbt())),
            Some(ClickEvent::Custom(data)) => self.custom_text(data),
            _ => {}
        }
    }

    fn custom_text(&mut self, value: &'a CustomData) {
        if let Payload::Nbt(nbt) = &value.payload {
            self.push(Node::Tag(nbt.as_nbt()));
        }
    }
}
