use crate::inventory::menu::MenuKind;

/// A menu kind with all-default handling and no special behavior.
#[derive(Debug)]
pub struct BasicKind;

// SAFETY: This Foton-owned key uniquely identifies the concrete menu kind
// within the process.
unsafe impl foton_utils::DowncastType for BasicKind {
    const TYPE_KEY: foton_utils::DowncastTypeKey =
        foton_utils::DowncastTypeKey::new("foton:menu/basic");
}

impl MenuKind for BasicKind {}
