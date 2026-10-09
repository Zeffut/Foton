use std::{error::Error, fmt};

use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use text_components::TextComponent;

use super::{
    SnbtErrorKind, compare_nbt, merge_nbt_compounds, nbt_compounds_equal,
    nbt_list_values as list_as_tags, parse_snbt_compound_argument, to_canonical_snbt,
};
use crate::translations;

/// Error returned when parsing an NBT path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NbtPathError {
    cursor: usize,
    kind: NbtPathErrorKind,
}

impl NbtPathError {
    const fn new(cursor: usize, kind: NbtPathErrorKind) -> Self {
        Self { cursor, kind }
    }

    /// Returns the byte cursor where parsing failed.
    #[must_use]
    pub const fn cursor(&self) -> usize {
        self.cursor
    }

    /// Returns the specific parse failure.
    #[must_use]
    pub const fn kind(&self) -> &NbtPathErrorKind {
        &self.kind
    }

    /// Returns the parse failure as a text component.
    #[must_use]
    pub fn component(&self) -> TextComponent {
        self.kind.component()
    }
}

impl fmt::Display for NbtPathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "NBT path parse error at byte {}: {}",
            self.cursor, self.kind
        )
    }
}

impl Error for NbtPathError {}

/// Specific reason why NBT path parsing failed.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NbtPathErrorKind {
    /// Non-whitespace input remained after a complete path.
    TrailingData,
    /// No path node was present.
    ExpectedPath,
    /// A path node used invalid syntax.
    InvalidNode,
    /// A grammar symbol was required at the cursor.
    ExpectedSymbol(char),
    /// A quoted path key was required.
    ExpectedQuotedString,
    /// A quoted path key was not terminated.
    UnclosedQuotedString,
    /// A quoted path key contained an unsupported escape.
    InvalidEscape(char),
    /// A list index was required.
    ExpectedIndex,
    /// A list index could not be parsed as an integer.
    InvalidIndex(String),
    /// An embedded compound pattern contained invalid SNBT.
    InvalidSnbt(SnbtErrorKind),
}

impl NbtPathErrorKind {
    fn component(&self) -> TextComponent {
        match self {
            Self::TrailingData => TextComponent::from(&translations::ARGUMENT_NBT_TRAILING),
            Self::ExpectedPath | Self::InvalidNode => {
                TextComponent::from(&translations::ARGUMENTS_NBTPATH_NODE_INVALID)
            }
            Self::ExpectedIndex => TextComponent::from(&translations::PARSING_INT_EXPECTED),
            Self::InvalidIndex(value) => translations::PARSING_INT_INVALID
                .message([value.to_owned()])
                .component(),
            Self::ExpectedSymbol(symbol) => translations::PARSING_EXPECTED
                .message([symbol.to_string()])
                .component(),
            Self::ExpectedQuotedString => {
                TextComponent::from(&translations::PARSING_QUOTE_EXPECTED_START)
            }
            Self::UnclosedQuotedString => {
                TextComponent::from(&translations::PARSING_QUOTE_EXPECTED_END)
            }
            Self::InvalidEscape(character) => translations::PARSING_QUOTE_ESCAPE
                .message([character.to_string()])
                .component(),
            Self::InvalidSnbt(kind) => kind.component(),
        }
    }
}

impl fmt::Display for NbtPathErrorKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TrailingData => formatter.write_str("trailing data"),
            Self::ExpectedPath => formatter.write_str("expected NBT path"),
            Self::InvalidNode => formatter.write_str("invalid NBT path node"),
            Self::ExpectedSymbol(symbol) => write!(formatter, "expected '{symbol}'"),
            Self::ExpectedQuotedString => formatter.write_str("expected quoted string"),
            Self::UnclosedQuotedString => formatter.write_str("unclosed quoted string"),
            Self::InvalidEscape(character) => write!(formatter, "invalid escape '{character}'"),
            Self::ExpectedIndex => formatter.write_str("expected list index"),
            Self::InvalidIndex(value) => write!(formatter, "invalid list index '{value}'"),
            Self::InvalidSnbt(kind) => fmt::Display::fmt(kind, formatter),
        }
    }
}

/// Error returned when mutating NBT through a path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NbtPathMutationError {
    /// An intermediate path node did not resolve to any tag.
    NothingFound(String),
    /// The inserted value would exceed vanilla's maximum NBT depth.
    TooDeep,
    /// An insert reached a tag that is not a list or array; carries its SNBT.
    ExpectedList(String),
    /// A merge reached a tag that is not a compound; carries its SNBT.
    ExpectedObject(String),
    /// An insert index fell outside the target list.
    InvalidIndex(i32),
}

impl NbtPathMutationError {
    /// Returns vanilla's message for this failure.
    #[must_use]
    pub fn component(&self) -> TextComponent {
        match self {
            Self::NothingFound(path) => translations::ARGUMENTS_NBTPATH_NOTHING_FOUND
                .message([path.clone()])
                .component(),
            Self::TooDeep => TextComponent::from(&translations::ARGUMENTS_NBTPATH_TOO_DEEP),
            Self::ExpectedList(tag) => translations::COMMANDS_DATA_MODIFY_EXPECTED_LIST
                .message([tag.clone()])
                .component(),
            Self::ExpectedObject(tag) => translations::COMMANDS_DATA_MODIFY_EXPECTED_OBJECT
                .message([tag.clone()])
                .component(),
            Self::InvalidIndex(index) => translations::COMMANDS_DATA_MODIFY_INVALID_INDEX
                .message([index.to_string()])
                .component(),
        }
    }
}

impl fmt::Display for NbtPathMutationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NothingFound(path) => write!(f, "nothing found at NBT path '{path}'"),
            Self::TooDeep => write!(f, "NBT path mutation would exceed maximum depth"),
            Self::ExpectedList(tag) => write!(f, "expected a list, got {tag}"),
            Self::ExpectedObject(tag) => write!(f, "expected an object, got {tag}"),
            Self::InvalidIndex(index) => write!(f, "invalid list index {index}"),
        }
    }
}

impl Error for NbtPathMutationError {}

/// Parsed vanilla NBT path.
#[derive(Clone, Debug, PartialEq)]
pub struct NbtPath {
    original: String,
    nodes: Vec<NbtPathNodeEntry>,
}

#[derive(Clone, Debug, PartialEq)]
struct NbtPathNodeEntry {
    node: NbtPathNode,
    original_end: usize,
}

impl NbtPath {
    /// Returns the path as written in command input.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.original
    }

    /// Returns cloned tags selected by this path.
    #[must_use]
    pub fn get(&self, tag: &NbtTag) -> Vec<NbtTag> {
        let mut tags = vec![tag.clone()];
        for entry in &self.nodes {
            tags = entry.node.get(&tags);
            if tags.is_empty() {
                break;
            }
        }
        tags
    }

    /// Returns the number of tags matched by this path.
    #[must_use]
    pub fn count_matching(&self, tag: &NbtTag) -> usize {
        let mut tags = vec![tag.clone()];
        for entry in &self.nodes {
            tags = entry.node.get(&tags);
            if tags.is_empty() {
                return 0;
            }
        }
        tags.len()
    }

    /// Returns cloned tags selected by this path, failing like vanilla's
    /// `NbtPath.get` when a node selects nothing.
    ///
    /// # Errors
    ///
    /// Returns [`NbtPathMutationError::NothingFound`] naming the path up to the
    /// first node that matched no tag.
    pub fn try_get(&self, tag: &NbtTag) -> Result<Vec<NbtTag>, NbtPathMutationError> {
        let mut tags = vec![tag.clone()];
        for entry in &self.nodes {
            tags = entry.node.get(&tags);
            if tags.is_empty() {
                return Err(NbtPathMutationError::NothingFound(
                    self.original[..entry.original_end].to_owned(),
                ));
            }
        }
        Ok(tags)
    }

    /// Vanilla parity: `NbtPath.isTooDeep`.
    #[must_use]
    pub fn is_too_deep(tag: &NbtTag, depth: usize) -> bool {
        is_too_deep(tag, depth)
    }

    /// Removes every tag selected by this path and returns how many went.
    ///
    /// Vanilla parity: `NbtPath.remove`.
    pub fn remove(&self, tag: &mut NbtTag) -> usize {
        self.remove_at(tag, 0)
    }

    fn remove_at(&self, tag: &mut NbtTag, node_index: usize) -> usize {
        let entry = &self.nodes[node_index];
        if node_index + 1 == self.nodes.len() {
            return entry.node.remove_tag(tag);
        }
        let mut removed = 0;
        // The visitor below never fails, so neither does the walk.
        let _ = entry.node.visit_existing(tag, |child| {
            removed += self.remove_at(child, node_index + 1);
            Ok(true)
        });
        removed
    }

    /// Visits every tag this path selects, creating missing parents and a
    /// `new_value` leaf the way vanilla's `NbtPath.getOrCreate` does.
    fn visit_or_create(
        &self,
        tag: &mut NbtTag,
        node_index: usize,
        new_value: &NbtTag,
        visitor: &mut dyn FnMut(&mut NbtTag) -> Result<(), NbtPathMutationError>,
    ) -> Result<bool, NbtPathMutationError> {
        let entry = &self.nodes[node_index];
        if node_index + 1 == self.nodes.len() {
            return entry.node.get_or_create_tag(tag, new_value, |leaf| {
                visitor(leaf)?;
                Ok(true)
            });
        }

        let preferred_child = self.nodes[node_index + 1]
            .node
            .create_preferred_parent_tag();
        let found = entry
            .node
            .get_or_create_tag(tag, &preferred_child, |child| {
                self.visit_or_create(child, node_index + 1, new_value, visitor)
            })?;
        if !found {
            return Err(NbtPathMutationError::NothingFound(
                self.original[..entry.original_end].to_owned(),
            ));
        }
        Ok(found)
    }

    /// Inserts `to_insert` into every list this path selects, creating the
    /// list when it is missing. Returns the number of lists that changed.
    ///
    /// `index` is vanilla's: negative counts back from the end, so `-1`
    /// appends. Vanilla parity: `NbtPath.insert`.
    ///
    /// # Errors
    ///
    /// Fails when a value is too deep, a selected tag is not a list or array,
    /// or `index` falls outside a list.
    pub fn insert(
        &self,
        index: i32,
        target: &mut NbtTag,
        to_insert: &[NbtTag],
    ) -> Result<usize, NbtPathMutationError> {
        if to_insert
            .iter()
            .any(|tag| is_too_deep(tag, self.nodes.len()))
        {
            return Err(NbtPathMutationError::TooDeep);
        }

        let mut modified_count = 0;
        self.visit_or_create(
            target,
            0,
            &NbtTag::List(NbtList::default()),
            &mut |collection| {
                let Some(len) = collection_len(collection) else {
                    return Err(NbtPathMutationError::ExpectedList(snbt_of(collection)));
                };
                let mut actual_index = if index < 0 {
                    len as i64 + i64::from(index) + 1
                } else {
                    i64::from(index)
                };
                let mut modified = false;
                for source in to_insert {
                    match insert_collection_element(collection, actual_index, source.clone()) {
                        InsertOutcome::Inserted => {
                            actual_index += 1;
                            modified = true;
                        }
                        InsertOutcome::Rejected => {}
                        InsertOutcome::OutOfBounds => {
                            return Err(NbtPathMutationError::InvalidIndex(actual_index as i32));
                        }
                    }
                }
                modified_count += usize::from(modified);
                Ok(())
            },
        )?;
        Ok(modified_count)
    }

    /// Merges `source` into every compound this path selects, creating the
    /// compound when it is missing. Returns the number of compounds that
    /// changed.
    ///
    /// Vanilla parity: the `merge` operation of `DataCommands`, built on
    /// `NbtPath.getOrCreate`.
    ///
    /// # Errors
    ///
    /// Fails when a selected tag is not a compound.
    pub fn merge_compound(
        &self,
        target: &mut NbtTag,
        source: &NbtCompound,
    ) -> Result<usize, NbtPathMutationError> {
        let mut changed_count = 0;
        self.visit_or_create(
            target,
            0,
            &NbtTag::Compound(NbtCompound::new()),
            &mut |tag| {
                let NbtTag::Compound(compound) = tag else {
                    return Err(NbtPathMutationError::ExpectedObject(snbt_of(tag)));
                };
                let original = compound.clone();
                merge_nbt_compounds(compound, source);
                changed_count += usize::from(!nbt_compounds_equal(&original, compound));
                Ok(())
            },
        )?;
        Ok(changed_count)
    }

    /// Sets every tag selected by this path to `value`.
    ///
    /// Missing intermediate compound/list parents are created the same way
    /// vanilla's `NbtPathArgument.NbtPath#set` creates them. The return value is
    /// the number of changed target tags.
    ///
    /// # Errors
    ///
    /// Returns an error if an intermediate path node cannot resolve or if the
    /// inserted value would exceed vanilla's maximum NBT depth.
    pub fn set(&self, tag: &mut NbtTag, value: NbtTag) -> Result<usize, NbtPathMutationError> {
        if is_too_deep(&value, self.nodes.len()) {
            return Err(NbtPathMutationError::TooDeep);
        }

        let Some((last, parents)) = self.nodes.split_last() else {
            return Ok(0);
        };
        if parents.is_empty() {
            return Ok(last.node.set_tag(tag, &value));
        }

        self.set_at(tag, 0, &value).map(|outcome| outcome.modified)
    }

    fn set_at(
        &self,
        tag: &mut NbtTag,
        node_index: usize,
        value: &NbtTag,
    ) -> Result<SetOutcome, NbtPathMutationError> {
        let entry = &self.nodes[node_index];
        if node_index + 1 == self.nodes.len() {
            return Ok(SetOutcome {
                found: true,
                modified: entry.node.set_tag(tag, value),
            });
        }

        let preferred_child = self.nodes[node_index + 1]
            .node
            .create_preferred_parent_tag();
        let mut modified = 0;
        let mut deeper_found = false;
        let found = entry
            .node
            .get_or_create_tag(tag, &preferred_child, |child| {
                let outcome = self.set_at(child, node_index + 1, value)?;
                modified += outcome.modified;
                deeper_found |= outcome.found;
                Ok(outcome.found)
            })?;
        if !found {
            return Err(NbtPathMutationError::NothingFound(
                self.original[..entry.original_end].to_owned(),
            ));
        }

        Ok(SetOutcome {
            found: deeper_found,
            modified,
        })
    }
}

struct SetOutcome {
    found: bool,
    modified: usize,
}

/// Parses one complete NBT path.
///
/// # Errors
///
/// Returns an error when the input is not a valid NBT path or has trailing data.
pub fn parse_nbt_path(input: &str) -> Result<NbtPath, NbtPathError> {
    let (path, cursor) = parse_nbt_path_argument(input)?;
    if cursor != input.len() {
        return Err(NbtPathError::new(cursor, NbtPathErrorKind::TrailingData));
    }
    Ok(path)
}

/// Parses one NBT path and returns the byte cursor consumed by it.
///
/// # Errors
///
/// Returns an error when the input does not start with a valid NBT path.
pub fn parse_nbt_path_argument(input: &str) -> Result<(NbtPath, usize), NbtPathError> {
    let mut parser = Parser::new(input);
    let path = parser.parse()?;
    Ok((path, parser.cursor))
}

#[derive(Clone, Debug, PartialEq)]
enum NbtPathNode {
    CompoundChild(String),
    MatchObject { name: String, pattern: NbtCompound },
    MatchRootObject(NbtCompound),
    AllElements,
    IndexedElement(i32),
    MatchElement(NbtCompound),
}

impl NbtPathNode {
    fn get(&self, input: &[NbtTag]) -> Vec<NbtTag> {
        let mut output = Vec::new();
        for tag in input {
            match self {
                Self::CompoundChild(name) => {
                    if let NbtTag::Compound(compound) = tag
                        && let Some(child) = compound.get(name)
                    {
                        output.push(child.clone());
                    }
                }
                Self::MatchObject { name, pattern } => {
                    if let NbtTag::Compound(compound) = tag
                        && let Some(child) = compound.get(name)
                        && compound_pattern_matches(pattern, child)
                    {
                        output.push(child.clone());
                    }
                }
                Self::MatchRootObject(pattern) => {
                    if compound_pattern_matches(pattern, tag) {
                        output.push(tag.clone());
                    }
                }
                Self::AllElements => {
                    output.extend(collection_elements(tag));
                }
                Self::IndexedElement(index) => {
                    let elements = collection_elements(tag);
                    if let Some(element) = indexed_element(&elements, *index) {
                        output.push(element);
                    }
                }
                Self::MatchElement(pattern) => {
                    output.extend(
                        collection_elements(tag)
                            .into_iter()
                            .filter(|tag| compound_pattern_matches(pattern, tag)),
                    );
                }
            }
        }
        output
    }

    fn get_or_create_tag(
        &self,
        parent: &mut NbtTag,
        child: &NbtTag,
        mut visitor: impl FnMut(&mut NbtTag) -> Result<bool, NbtPathMutationError>,
    ) -> Result<bool, NbtPathMutationError> {
        match self {
            Self::CompoundChild(name) => {
                let NbtTag::Compound(compound) = parent else {
                    return Ok(false);
                };
                ensure_compound_child(compound, name, child.clone());
                let Some(tag) = compound.get_mut(name) else {
                    return Ok(false);
                };
                visitor(tag)
            }
            Self::MatchObject { name, pattern } => {
                let NbtTag::Compound(compound) = parent else {
                    return Ok(false);
                };
                if !compound.contains(name) {
                    compound.insert(name.as_str(), NbtTag::Compound(pattern.clone()));
                }
                let Some(tag) = compound.get_mut(name) else {
                    return Ok(false);
                };
                if compound_pattern_matches(pattern, tag) {
                    visitor(tag)
                } else {
                    Ok(false)
                }
            }
            Self::MatchRootObject(pattern) => {
                if compound_pattern_matches(pattern, parent) {
                    visitor(parent)
                } else {
                    Ok(false)
                }
            }
            Self::AllElements => {
                ensure_non_empty_collection(parent, child);
                visit_all_collection_elements(parent, visitor)
            }
            Self::IndexedElement(index) => {
                visit_indexed_collection_element(parent, *index, visitor)
            }
            Self::MatchElement(pattern) => {
                if let NbtTag::List(list) = parent {
                    ensure_matching_list_element(list, pattern);
                }
                visit_matching_list_elements(parent, pattern, visitor)
            }
        }
    }

    /// Visits the tags this node selects under `parent`, creating nothing.
    ///
    /// Vanilla parity: `Node.getTag`, with mutable access so `remove` can reach
    /// the parents of the tag it deletes.
    fn visit_existing(
        &self,
        parent: &mut NbtTag,
        mut visitor: impl FnMut(&mut NbtTag) -> Result<bool, NbtPathMutationError>,
    ) -> Result<bool, NbtPathMutationError> {
        match self {
            Self::CompoundChild(name) => {
                let NbtTag::Compound(compound) = parent else {
                    return Ok(false);
                };
                compound.get_mut(name).map_or(Ok(false), visitor)
            }
            Self::MatchObject { name, pattern } => {
                let NbtTag::Compound(compound) = parent else {
                    return Ok(false);
                };
                match compound.get_mut(name) {
                    Some(tag) if compound_pattern_matches(pattern, tag) => visitor(tag),
                    _ => Ok(false),
                }
            }
            Self::MatchRootObject(pattern) => {
                if compound_pattern_matches(pattern, parent) {
                    visitor(parent)
                } else {
                    Ok(false)
                }
            }
            Self::AllElements => visit_all_collection_elements(parent, visitor),
            Self::IndexedElement(index) => {
                visit_indexed_collection_element(parent, *index, visitor)
            }
            Self::MatchElement(pattern) => visit_matching_list_elements(parent, pattern, visitor),
        }
    }

    /// Vanilla parity: `Node.removeTag`. Returns the number of removed tags.
    fn remove_tag(&self, parent: &mut NbtTag) -> usize {
        match self {
            Self::CompoundChild(name) => {
                let NbtTag::Compound(compound) = parent else {
                    return 0;
                };
                usize::from(compound.remove(name).is_some())
            }
            Self::MatchObject { name, pattern } => {
                let NbtTag::Compound(compound) = parent else {
                    return 0;
                };
                if compound
                    .get(name)
                    .is_none_or(|current| !compound_pattern_matches(pattern, current))
                {
                    return 0;
                }
                usize::from(compound.remove(name).is_some())
            }
            Self::MatchRootObject(_) => 0,
            Self::AllElements => {
                let len = collection_len(parent).unwrap_or(0);
                clear_collection(parent);
                len
            }
            Self::IndexedElement(index) => {
                let Some(len) = collection_len(parent) else {
                    return 0;
                };
                let Some(actual_index) = actual_collection_index(len, *index) else {
                    return 0;
                };
                usize::from(remove_collection_element(parent, actual_index))
            }
            Self::MatchElement(pattern) => {
                let NbtTag::List(list) = parent else {
                    return 0;
                };
                let mut elements = list_as_tags(list);
                let before = elements.len();
                elements.retain(|element| !compound_pattern_matches(pattern, element));
                let removed = before - elements.len();
                if removed > 0 {
                    *list = NbtList::from(elements);
                }
                removed
            }
        }
    }

    fn create_preferred_parent_tag(&self) -> NbtTag {
        match self {
            Self::CompoundChild(_) | Self::MatchObject { .. } | Self::MatchRootObject(_) => {
                NbtTag::Compound(NbtCompound::new())
            }
            Self::AllElements | Self::IndexedElement(_) | Self::MatchElement(_) => {
                NbtTag::List(NbtList::default())
            }
        }
    }

    fn set_tag(&self, parent: &mut NbtTag, value: &NbtTag) -> usize {
        match self {
            Self::CompoundChild(name) => set_compound_child(parent, name, value),
            Self::MatchObject { name, pattern } => {
                set_matching_compound_child(parent, name, pattern, value)
            }
            Self::MatchRootObject(_) => 0,
            Self::AllElements => set_all_collection_elements(parent, value),
            Self::IndexedElement(index) => set_indexed_collection_element(parent, *index, value),
            Self::MatchElement(pattern) => set_matching_list_elements(parent, pattern, value),
        }
    }
}

/// The tag as SNBT, which is what vanilla's `Tag.toString()` feeds its messages.
fn snbt_of(tag: &NbtTag) -> String {
    to_canonical_snbt(tag).unwrap_or_default()
}

fn compound_pattern_matches(pattern: &NbtCompound, tag: &NbtTag) -> bool {
    compare_nbt(Some(&NbtTag::Compound(pattern.clone())), Some(tag), true)
}

fn ensure_compound_child(compound: &mut NbtCompound, name: &str, child: NbtTag) {
    if !compound.contains(name) {
        compound.insert(name, child);
    }
}

fn set_compound_child(parent: &mut NbtTag, name: &str, value: &NbtTag) -> usize {
    let NbtTag::Compound(compound) = parent else {
        return 0;
    };

    if compound.get(name).is_some_and(|current| current == value) {
        return 0;
    }
    if let Some(current) = compound.get_mut(name) {
        *current = value.clone();
    } else {
        compound.insert(name, value.clone());
    }
    1
}

fn set_matching_compound_child(
    parent: &mut NbtTag,
    name: &str,
    pattern: &NbtCompound,
    value: &NbtTag,
) -> usize {
    let NbtTag::Compound(compound) = parent else {
        return 0;
    };
    let Some(current) = compound.get_mut(name) else {
        return 0;
    };
    if !compound_pattern_matches(pattern, current) || current == value {
        return 0;
    }

    *current = value.clone();
    1
}

fn visit_all_collection_elements(
    parent: &mut NbtTag,
    mut visitor: impl FnMut(&mut NbtTag) -> Result<bool, NbtPathMutationError>,
) -> Result<bool, NbtPathMutationError> {
    let Some(mut elements) = collection_elements_for_mutation(parent) else {
        return Ok(false);
    };
    if elements.is_empty() {
        return Ok(false);
    }

    let mut found = false;
    let mut changed = false;
    for element in &mut elements {
        let before = element.clone();
        found |= visitor(element)?;
        changed |= *element != before;
    }
    if changed {
        replace_collection(parent, elements);
    }
    Ok(found)
}

fn visit_indexed_collection_element(
    parent: &mut NbtTag,
    index: i32,
    mut visitor: impl FnMut(&mut NbtTag) -> Result<bool, NbtPathMutationError>,
) -> Result<bool, NbtPathMutationError> {
    let Some(mut elements) = collection_elements_for_mutation(parent) else {
        return Ok(false);
    };
    let Some(actual_index) = actual_collection_index(elements.len(), index) else {
        return Ok(false);
    };

    let before = elements[actual_index].clone();
    let found = visitor(&mut elements[actual_index])?;
    if elements[actual_index] != before {
        replace_collection(parent, elements);
    }
    Ok(found)
}

fn visit_matching_list_elements(
    parent: &mut NbtTag,
    pattern: &NbtCompound,
    mut visitor: impl FnMut(&mut NbtTag) -> Result<bool, NbtPathMutationError>,
) -> Result<bool, NbtPathMutationError> {
    let NbtTag::List(list) = parent else {
        return Ok(false);
    };

    let mut elements = list_as_tags(list);
    let mut found = false;
    let mut changed = false;
    for element in elements
        .iter_mut()
        .filter(|element| compound_pattern_matches(pattern, element))
    {
        let before = element.clone();
        found |= visitor(element)?;
        changed |= *element != before;
    }
    if changed {
        *list = NbtList::from(elements);
    }
    Ok(found)
}

fn ensure_non_empty_collection(parent: &mut NbtTag, child: &NbtTag) {
    if collection_len(parent) != Some(0) {
        return;
    }
    add_collection_element(parent, child.clone());
}

fn ensure_matching_list_element(list: &mut NbtList, pattern: &NbtCompound) {
    let mut elements = list_as_tags(list);
    if elements
        .iter()
        .any(|element| compound_pattern_matches(pattern, element))
    {
        return;
    }

    elements.push(NbtTag::Compound(pattern.clone()));
    *list = NbtList::from(elements);
}

fn set_all_collection_elements(parent: &mut NbtTag, value: &NbtTag) -> usize {
    let Some(len) = collection_len(parent) else {
        return 0;
    };
    if len == 0 {
        add_collection_element(parent, value.clone());
        return 1;
    }

    let Some(elements) = collection_elements_for_mutation(parent) else {
        return 0;
    };
    let changed_count = elements.iter().filter(|element| *element != value).count();
    if changed_count == 0 {
        return 0;
    }

    if replace_collection_with_repeated(parent, len, value) {
        changed_count
    } else {
        clear_collection(parent);
        0
    }
}

fn set_indexed_collection_element(parent: &mut NbtTag, index: i32, value: &NbtTag) -> usize {
    let Some(len) = collection_len(parent) else {
        return 0;
    };
    let Some(actual_index) = actual_collection_index(len, index) else {
        return 0;
    };
    if collection_element(parent, actual_index).is_some_and(|current| &current == value) {
        return 0;
    }

    usize::from(set_collection_element(parent, actual_index, value.clone()))
}

fn set_matching_list_elements(parent: &mut NbtTag, pattern: &NbtCompound, value: &NbtTag) -> usize {
    let NbtTag::List(list) = parent else {
        return 0;
    };
    if list_is_empty(list) {
        return usize::from(add_list_element(list, value.clone()));
    }

    let mut elements = list_as_tags(list);
    let mut changed = 0;
    for element in &mut elements {
        if !compound_pattern_matches(pattern, element) || element == value {
            continue;
        }
        *element = value.clone();
        changed += 1;
    }
    if changed > 0 {
        *list = NbtList::from(elements);
    }
    changed
}

const fn collection_len(tag: &NbtTag) -> Option<usize> {
    match tag {
        NbtTag::List(list) => Some(list_len(list)),
        NbtTag::ByteArray(values) => Some(values.len()),
        NbtTag::IntArray(values) => Some(values.len()),
        NbtTag::LongArray(values) => Some(values.len()),
        _ => None,
    }
}

const fn list_len(list: &NbtList) -> usize {
    match list {
        NbtList::Empty => 0,
        NbtList::Byte(values) => values.len(),
        NbtList::Short(values) => values.len(),
        NbtList::Int(values) => values.len(),
        NbtList::Long(values) => values.len(),
        NbtList::Float(values) => values.len(),
        NbtList::Double(values) => values.len(),
        NbtList::ByteArray(values) => values.len(),
        NbtList::String(values) => values.len(),
        NbtList::List(values) => values.len(),
        NbtList::Compound(values) => values.len(),
        NbtList::IntArray(values) => values.len(),
        NbtList::LongArray(values) => values.len(),
    }
}

const fn list_is_empty(list: &NbtList) -> bool {
    list_len(list) == 0
}

fn actual_collection_index(len: usize, index: i32) -> Option<usize> {
    let actual_index = if index < 0 {
        len.checked_add_signed(index as isize)?
    } else {
        usize::try_from(index).ok()?
    };
    (actual_index < len).then_some(actual_index)
}

fn collection_element(tag: &NbtTag, index: usize) -> Option<NbtTag> {
    match tag {
        NbtTag::List(list) => list_as_tags(list).get(index).cloned(),
        NbtTag::ByteArray(values) => values.get(index).map(|value| NbtTag::Byte(*value as i8)),
        NbtTag::IntArray(values) => values.get(index).copied().map(NbtTag::Int),
        NbtTag::LongArray(values) => values.get(index).copied().map(NbtTag::Long),
        _ => None,
    }
}

fn collection_elements_for_mutation(tag: &NbtTag) -> Option<Vec<NbtTag>> {
    match tag {
        NbtTag::List(list) => Some(list_as_tags(list)),
        NbtTag::ByteArray(values) => Some(
            values
                .iter()
                .map(|value| NbtTag::Byte(*value as i8))
                .collect(),
        ),
        NbtTag::IntArray(values) => Some(values.iter().copied().map(NbtTag::Int).collect()),
        NbtTag::LongArray(values) => Some(values.iter().copied().map(NbtTag::Long).collect()),
        _ => None,
    }
}

fn replace_collection(parent: &mut NbtTag, elements: Vec<NbtTag>) {
    match parent {
        NbtTag::List(list) => *list = NbtList::from(elements),
        NbtTag::ByteArray(values) => {
            if let Some(replacement) = tags_to_byte_array(elements) {
                *values = replacement;
            }
        }
        NbtTag::IntArray(values) => {
            if let Some(replacement) = tags_to_int_array(elements) {
                *values = replacement;
            }
        }
        NbtTag::LongArray(values) => {
            if let Some(replacement) = tags_to_long_array(elements) {
                *values = replacement;
            }
        }
        _ => {}
    }
}

fn replace_collection_with_repeated(parent: &mut NbtTag, len: usize, value: &NbtTag) -> bool {
    match parent {
        NbtTag::List(list) => {
            *list = NbtList::from(vec![value.clone(); len]);
            true
        }
        NbtTag::ByteArray(values) => {
            let Some(value) = nbt_byte_value(value) else {
                return false;
            };
            *values = vec![value as u8; len];
            true
        }
        NbtTag::IntArray(values) => {
            let Some(value) = nbt_int_value(value) else {
                return false;
            };
            *values = vec![value; len];
            true
        }
        NbtTag::LongArray(values) => {
            let Some(value) = nbt_long_value(value) else {
                return false;
            };
            *values = vec![value; len];
            true
        }
        _ => false,
    }
}

fn clear_collection(parent: &mut NbtTag) {
    match parent {
        NbtTag::List(list) => clear_list(list),
        NbtTag::ByteArray(values) => values.clear(),
        NbtTag::IntArray(values) => values.clear(),
        NbtTag::LongArray(values) => values.clear(),
        _ => {}
    }
}

fn clear_list(list: &mut NbtList) {
    match list {
        NbtList::Empty => {}
        NbtList::Byte(values) => values.clear(),
        NbtList::Short(values) => values.clear(),
        NbtList::Int(values) => values.clear(),
        NbtList::Long(values) => values.clear(),
        NbtList::Float(values) => values.clear(),
        NbtList::Double(values) => values.clear(),
        NbtList::ByteArray(values) => values.clear(),
        NbtList::String(values) => values.clear(),
        NbtList::List(values) => values.clear(),
        NbtList::Compound(values) => values.clear(),
        NbtList::IntArray(values) => values.clear(),
        NbtList::LongArray(values) => values.clear(),
    }
}

fn add_collection_element(parent: &mut NbtTag, value: NbtTag) -> bool {
    match parent {
        NbtTag::List(list) => add_list_element(list, value),
        NbtTag::ByteArray(values) => {
            let Some(value) = nbt_byte_value(&value) else {
                return false;
            };
            values.push(value as u8);
            true
        }
        NbtTag::IntArray(values) => {
            let Some(value) = nbt_int_value(&value) else {
                return false;
            };
            values.push(value);
            true
        }
        NbtTag::LongArray(values) => {
            let Some(value) = nbt_long_value(&value) else {
                return false;
            };
            values.push(value);
            true
        }
        _ => false,
    }
}

/// Vanilla parity: `CollectionTag.remove(int)`.
fn remove_collection_element(parent: &mut NbtTag, index: usize) -> bool {
    match parent {
        NbtTag::List(list) => {
            let mut elements = list_as_tags(list);
            if index >= elements.len() {
                return false;
            }
            elements.remove(index);
            *list = NbtList::from(elements);
            true
        }
        NbtTag::ByteArray(values) if index < values.len() => {
            values.remove(index);
            true
        }
        NbtTag::IntArray(values) if index < values.len() => {
            values.remove(index);
            true
        }
        NbtTag::LongArray(values) if index < values.len() => {
            values.remove(index);
            true
        }
        _ => false,
    }
}

/// Outcome of a `CollectionTag.addTag(index, tag)` call.
enum InsertOutcome {
    Inserted,
    /// The element's type does not fit the collection; vanilla returns `false`.
    Rejected,
    /// `index` is outside `0..=len`; vanilla throws `IndexOutOfBoundsException`.
    OutOfBounds,
}

/// Vanilla parity: `CollectionTag.addTag(int, Tag)` for lists and numeric arrays.
fn insert_collection_element(parent: &mut NbtTag, index: i64, value: NbtTag) -> InsertOutcome {
    let Some(len) = collection_len(parent) else {
        return InsertOutcome::Rejected;
    };
    let Ok(index) = usize::try_from(index) else {
        return InsertOutcome::OutOfBounds;
    };
    match parent {
        NbtTag::List(list) => {
            let mut elements = list_as_tags(list);
            // `ListTag.updateType`: an empty list takes any type, otherwise
            // the element must match the list's.
            if elements
                .first()
                .is_some_and(|first| first.id() != value.id())
            {
                return InsertOutcome::Rejected;
            }
            if index > len {
                return InsertOutcome::OutOfBounds;
            }
            elements.insert(index, value);
            *list = NbtList::from(elements);
            InsertOutcome::Inserted
        }
        NbtTag::ByteArray(values) => {
            let Some(value) = nbt_byte_value(&value) else {
                return InsertOutcome::Rejected;
            };
            if index > len {
                return InsertOutcome::OutOfBounds;
            }
            values.insert(index, value as u8);
            InsertOutcome::Inserted
        }
        NbtTag::IntArray(values) => {
            let Some(value) = nbt_int_value(&value) else {
                return InsertOutcome::Rejected;
            };
            if index > len {
                return InsertOutcome::OutOfBounds;
            }
            values.insert(index, value);
            InsertOutcome::Inserted
        }
        NbtTag::LongArray(values) => {
            let Some(value) = nbt_long_value(&value) else {
                return InsertOutcome::Rejected;
            };
            if index > len {
                return InsertOutcome::OutOfBounds;
            }
            values.insert(index, value);
            InsertOutcome::Inserted
        }
        _ => InsertOutcome::Rejected,
    }
}

fn add_list_element(list: &mut NbtList, value: NbtTag) -> bool {
    let mut elements = list_as_tags(list);
    elements.push(value);
    *list = NbtList::from(elements);
    true
}

fn set_collection_element(parent: &mut NbtTag, index: usize, value: NbtTag) -> bool {
    match parent {
        NbtTag::List(list) => set_list_element(list, index, value),
        NbtTag::ByteArray(values) => {
            let Some(value) = nbt_byte_value(&value) else {
                return false;
            };
            let Some(current) = values.get_mut(index) else {
                return false;
            };
            *current = value as u8;
            true
        }
        NbtTag::IntArray(values) => {
            let Some(value) = nbt_int_value(&value) else {
                return false;
            };
            let Some(current) = values.get_mut(index) else {
                return false;
            };
            *current = value;
            true
        }
        NbtTag::LongArray(values) => {
            let Some(value) = nbt_long_value(&value) else {
                return false;
            };
            let Some(current) = values.get_mut(index) else {
                return false;
            };
            *current = value;
            true
        }
        _ => false,
    }
}

fn set_list_element(list: &mut NbtList, index: usize, value: NbtTag) -> bool {
    let mut elements = list_as_tags(list);
    let Some(current) = elements.get_mut(index) else {
        return false;
    };
    *current = value;
    *list = NbtList::from(elements);
    true
}

fn tags_to_byte_array(tags: Vec<NbtTag>) -> Option<Vec<u8>> {
    tags.into_iter()
        .map(|tag| {
            let value = nbt_byte_value(&tag)?;
            Some(value as u8)
        })
        .collect()
}

fn tags_to_int_array(tags: Vec<NbtTag>) -> Option<Vec<i32>> {
    tags.into_iter().map(|tag| nbt_int_value(&tag)).collect()
}

fn tags_to_long_array(tags: Vec<NbtTag>) -> Option<Vec<i64>> {
    tags.into_iter().map(|tag| nbt_long_value(&tag)).collect()
}

const fn nbt_byte_value(tag: &NbtTag) -> Option<i8> {
    Some(match tag {
        NbtTag::Byte(value) => *value,
        NbtTag::Short(value) => *value as u8 as i8,
        NbtTag::Int(value) => *value as u8 as i8,
        NbtTag::Long(value) => *value as u8 as i8,
        NbtTag::Float(value) => value.floor() as i32 as u8 as i8,
        NbtTag::Double(value) => value.floor() as i32 as u8 as i8,
        _ => return None,
    })
}

fn nbt_int_value(tag: &NbtTag) -> Option<i32> {
    Some(match tag {
        NbtTag::Byte(value) => i32::from(*value),
        NbtTag::Short(value) => i32::from(*value),
        NbtTag::Int(value) => *value,
        NbtTag::Long(value) => *value as i32,
        NbtTag::Float(value) => value.floor() as i32,
        NbtTag::Double(value) => value.floor() as i32,
        _ => return None,
    })
}

fn nbt_long_value(tag: &NbtTag) -> Option<i64> {
    Some(match tag {
        NbtTag::Byte(value) => i64::from(*value),
        NbtTag::Short(value) => i64::from(*value),
        NbtTag::Int(value) => i64::from(*value),
        NbtTag::Long(value) => *value,
        NbtTag::Float(value) => *value as i64,
        NbtTag::Double(value) => value.floor() as i64,
        _ => return None,
    })
}

fn is_too_deep(tag: &NbtTag, depth: usize) -> bool {
    if depth >= 512 {
        return true;
    }

    match tag {
        NbtTag::Compound(compound) => compound.values().any(|child| is_too_deep(child, depth + 1)),
        NbtTag::List(list) => list_as_tags(list)
            .iter()
            .any(|child| is_too_deep(child, depth + 1)),
        _ => false,
    }
}

fn indexed_element(elements: &[NbtTag], index: i32) -> Option<NbtTag> {
    let actual_index = if index < 0 {
        elements.len().checked_add_signed(index as isize)?
    } else {
        usize::try_from(index).ok()?
    };
    elements.get(actual_index).cloned()
}

fn collection_elements(tag: &NbtTag) -> Vec<NbtTag> {
    match tag {
        NbtTag::List(list) => list_as_tags(list),
        NbtTag::ByteArray(values) => values
            .iter()
            .map(|value| NbtTag::Byte(*value as i8))
            .collect(),
        NbtTag::IntArray(values) => values.iter().copied().map(NbtTag::Int).collect(),
        NbtTag::LongArray(values) => values.iter().copied().map(NbtTag::Long).collect(),
        _ => Vec::new(),
    }
}

struct Parser<'a> {
    input: &'a str,
    cursor: usize,
}

impl<'a> Parser<'a> {
    const fn new(input: &'a str) -> Self {
        Self { input, cursor: 0 }
    }

    fn parse(&mut self) -> Result<NbtPath, NbtPathError> {
        let start = self.cursor;
        let mut nodes = Vec::new();
        let mut first_node = true;

        while self.can_read() && self.peek() != Some(' ') {
            let node = self.parse_node(first_node)?;
            nodes.push(NbtPathNodeEntry {
                node,
                original_end: self.cursor - start,
            });
            first_node = false;

            if self.can_read() {
                let Some(next) = self.peek() else {
                    break;
                };
                if next != ' ' && next != '[' && next != '{' {
                    self.expect_char('.')?;
                }
            }
        }

        if nodes.is_empty() {
            return Err(self.error(NbtPathErrorKind::ExpectedPath));
        }

        Ok(NbtPath {
            original: self.input[start..self.cursor].to_owned(),
            nodes,
        })
    }

    fn parse_node(&mut self, first_node: bool) -> Result<NbtPathNode, NbtPathError> {
        match self.peek() {
            Some('"' | '\'') => {
                let name = self.parse_quoted_string()?;
                self.read_object_node(name)
            }
            Some('[') => self.parse_element_node(),
            Some('{') => {
                if !first_node {
                    return Err(self.error(NbtPathErrorKind::InvalidNode));
                }
                let pattern = self.parse_compound_pattern()?;
                Ok(NbtPathNode::MatchRootObject(pattern))
            }
            Some(_) => {
                let name = self.parse_unquoted_name()?;
                self.read_object_node(name)
            }
            None => Err(self.error(NbtPathErrorKind::ExpectedPath)),
        }
    }

    fn read_object_node(&mut self, name: String) -> Result<NbtPathNode, NbtPathError> {
        if name.is_empty() {
            return Err(self.error(NbtPathErrorKind::ExpectedPath));
        }
        if self.peek() == Some('{') {
            let pattern = self.parse_compound_pattern()?;
            Ok(NbtPathNode::MatchObject { name, pattern })
        } else {
            Ok(NbtPathNode::CompoundChild(name))
        }
    }

    fn parse_element_node(&mut self) -> Result<NbtPathNode, NbtPathError> {
        self.expect_char('[')?;
        match self.peek() {
            Some('{') => {
                let pattern = self.parse_compound_pattern()?;
                self.expect_char(']')?;
                Ok(NbtPathNode::MatchElement(pattern))
            }
            Some(']') => {
                self.read();
                Ok(NbtPathNode::AllElements)
            }
            _ => {
                let index = self.parse_i32()?;
                self.expect_char(']')?;
                Ok(NbtPathNode::IndexedElement(index))
            }
        }
    }

    fn parse_compound_pattern(&mut self) -> Result<NbtCompound, NbtPathError> {
        let start = self.cursor;
        let (compound, consumed) =
            parse_snbt_compound_argument(&self.input[start..]).map_err(|error| {
                let cursor = error.cursor();
                NbtPathError::new(
                    start + cursor,
                    NbtPathErrorKind::InvalidSnbt(error.into_kind()),
                )
            })?;
        self.cursor += consumed;
        Ok(compound)
    }

    fn parse_unquoted_name(&mut self) -> Result<String, NbtPathError> {
        let start = self.cursor;
        while self.peek().is_some_and(is_allowed_in_unquoted_name) {
            self.read();
        }
        if self.cursor == start {
            return Err(self.error(NbtPathErrorKind::InvalidNode));
        }
        Ok(self.input[start..self.cursor].to_owned())
    }

    fn parse_quoted_string(&mut self) -> Result<String, NbtPathError> {
        let Some(terminator) = self.peek().filter(|ch| matches!(ch, '"' | '\'')) else {
            return Err(self.error(NbtPathErrorKind::ExpectedQuotedString));
        };
        self.read();

        let mut value = String::new();
        while let Some(ch) = self.read() {
            match ch {
                ch if ch == terminator => return Ok(value),
                '\\' => {
                    let escape_cursor = self.cursor;
                    let escaped = self
                        .read()
                        .ok_or_else(|| self.error(NbtPathErrorKind::UnclosedQuotedString))?;
                    if escaped != terminator && escaped != '\\' {
                        return Err(Self::error_at(
                            escape_cursor,
                            NbtPathErrorKind::InvalidEscape(escaped),
                        ));
                    }
                    value.push(escaped);
                }
                _ => value.push(ch),
            }
        }

        Err(self.error(NbtPathErrorKind::UnclosedQuotedString))
    }

    fn parse_i32(&mut self) -> Result<i32, NbtPathError> {
        let start = self.cursor;
        while self
            .peek()
            .is_some_and(|ch| ch.is_ascii_digit() || matches!(ch, '.' | '-'))
        {
            self.read();
        }
        let value = &self.input[start..self.cursor];
        if value.is_empty() {
            return Err(Self::error_at(start, NbtPathErrorKind::ExpectedIndex));
        }
        if let Ok(value) = value.parse() {
            return Ok(value);
        }
        let value = value.to_owned();
        self.cursor = start;
        Err(Self::error_at(start, NbtPathErrorKind::InvalidIndex(value)))
    }

    const fn can_read(&self) -> bool {
        self.cursor < self.input.len()
    }

    fn peek(&self) -> Option<char> {
        self.input[self.cursor..].chars().next()
    }

    fn read(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.cursor += ch.len_utf8();
        Some(ch)
    }

    fn expect_char(&mut self, expected: char) -> Result<(), NbtPathError> {
        if self.peek() == Some(expected) {
            self.read();
            Ok(())
        } else {
            Err(self.error(NbtPathErrorKind::ExpectedSymbol(expected)))
        }
    }

    const fn error(&self, kind: NbtPathErrorKind) -> NbtPathError {
        NbtPathError::new(self.cursor, kind)
    }

    const fn error_at(cursor: usize, kind: NbtPathErrorKind) -> NbtPathError {
        NbtPathError::new(cursor, kind)
    }
}

const fn is_allowed_in_unquoted_name(ch: char) -> bool {
    !matches!(ch, ' ' | '"' | '\'' | '[' | ']' | '.' | '{' | '}')
}

#[cfg(test)]
mod tests {
    use simdnbt::owned::{NbtCompound, NbtList, NbtTag};

    use super::*;

    fn compound(entries: impl IntoIterator<Item = (&'static str, NbtTag)>) -> NbtTag {
        let mut compound = NbtCompound::new();
        for (key, tag) in entries {
            compound.insert(key, tag);
        }
        NbtTag::Compound(compound)
    }

    fn list(entries: impl IntoIterator<Item = NbtTag>) -> NbtTag {
        NbtTag::List(NbtList::from(entries.into_iter().collect::<Vec<_>>()))
    }

    #[test]
    fn parses_path_argument_without_consuming_separator() {
        let (path, cursor) = parse_nbt_path_argument("foo.bar run").expect("path parses");

        assert_eq!(path.as_str(), "foo.bar");
        assert_eq!(cursor, 7);
    }

    #[test]
    fn counts_compound_child_matches() {
        let path = parse_nbt_path("foo.bar").expect("path parses");
        let tag = compound([("foo", compound([("bar", NbtTag::Int(3))]))]);

        assert_eq!(path.count_matching(&tag), 1);
        assert_eq!(path.get(&tag), vec![NbtTag::Int(3)]);
    }

    #[test]
    fn counts_list_index_and_wildcard_matches() {
        let path = parse_nbt_path("items[1].id").expect("path parses");
        let wildcard = parse_nbt_path("items[].id").expect("path parses");
        let tag = compound([(
            "items",
            list([
                compound([("id", NbtTag::String("first".into()))]),
                compound([("id", NbtTag::String("second".into()))]),
            ]),
        )]);

        assert_eq!(path.get(&tag), vec![NbtTag::String("second".into())]);
        assert_eq!(wildcard.count_matching(&tag), 2);
    }

    #[test]
    fn negative_indices_select_from_end() {
        let path = parse_nbt_path("items[-1]").expect("path parses");
        let tag = compound([(
            "items",
            list([NbtTag::Int(1), NbtTag::Int(2), NbtTag::Int(3)]),
        )]);

        assert_eq!(path.get(&tag), vec![NbtTag::Int(3)]);
    }

    #[test]
    fn predicate_nodes_use_partial_compound_matching() {
        let path = parse_nbt_path("items[{id:\"minecraft:stone\"}].Count").expect("path parses");
        let tag = compound([(
            "items",
            list([
                compound([
                    ("id", NbtTag::String("minecraft:dirt".into())),
                    ("Count", NbtTag::Byte(1)),
                ]),
                compound([
                    ("id", NbtTag::String("minecraft:stone".into())),
                    ("Count", NbtTag::Byte(4)),
                    ("Slot", NbtTag::Byte(0)),
                ]),
            ]),
        )]);

        assert_eq!(path.get(&tag), vec![NbtTag::Byte(4)]);
    }

    #[test]
    fn root_predicate_matches_root_compound() {
        let path = parse_nbt_path("{id:\"minecraft:barrel\"}").expect("path parses");
        let tag = compound([
            ("id", NbtTag::String("minecraft:barrel".into())),
            ("x", NbtTag::Int(4)),
        ]);

        assert_eq!(path.count_matching(&tag), 1);
    }

    #[test]
    fn preserves_embedded_snbt_error_kind() {
        let error = parse_nbt_path("items[{id:}]").expect_err("invalid pattern should fail");

        assert_eq!(
            error.kind(),
            &NbtPathErrorKind::InvalidSnbt(SnbtErrorKind::ExpectedValue)
        );
        assert_eq!(
            error.component(),
            TextComponent::from(&translations::SNBT_PARSER_EXPECTED_UNQUOTED_STRING)
        );
    }

    #[test]
    fn quoted_key_errors_use_brigadier_cursors() {
        let unclosed = parse_nbt_path(r#""items"#).expect_err("unclosed key should fail");
        assert_eq!(unclosed.cursor(), r#""items"#.len());
        assert_eq!(unclosed.kind(), &NbtPathErrorKind::UnclosedQuotedString);

        let invalid_escape =
            parse_nbt_path(r#""a\q""#).expect_err("invalid key escape should fail");
        assert_eq!(invalid_escape.cursor(), r#""a\"#.len());
        assert_eq!(invalid_escape.kind(), &NbtPathErrorKind::InvalidEscape('q'));
    }

    #[test]
    fn index_errors_use_brigadier_integer_components() {
        let expected = parse_nbt_path("items[x]").expect_err("missing integer should fail");
        assert_eq!(expected.cursor(), "items[".len());
        assert_eq!(expected.kind(), &NbtPathErrorKind::ExpectedIndex);
        assert_eq!(
            expected.component(),
            TextComponent::from(&translations::PARSING_INT_EXPECTED)
        );

        for value in ["-", "1.2", "999999999999999999999"] {
            let input = format!("items[{value}]");
            let invalid = parse_nbt_path(&input).expect_err("invalid integer should fail");
            assert_eq!(invalid.cursor(), "items[".len());
            assert_eq!(
                invalid.kind(),
                &NbtPathErrorKind::InvalidIndex(value.to_owned())
            );
            assert_eq!(
                invalid.component(),
                translations::PARSING_INT_INVALID
                    .message([value.to_owned()])
                    .component()
            );
        }
    }

    #[test]
    fn set_creates_missing_compound_parents() {
        let path = parse_nbt_path("foo.bar").expect("path parses");
        let mut tag = compound([]);

        assert_eq!(
            path.set(&mut tag, NbtTag::Int(7))
                .expect("path set should succeed"),
            1
        );
        assert_eq!(path.get(&tag), vec![NbtTag::Int(7)]);
    }

    #[test]
    fn set_updates_all_list_elements() {
        let path = parse_nbt_path("items[].Count").expect("path parses");
        let mut tag = compound([(
            "items",
            list([
                compound([("Count", NbtTag::Byte(1))]),
                compound([("Count", NbtTag::Byte(2))]),
            ]),
        )]);

        assert_eq!(
            path.set(&mut tag, NbtTag::Byte(4))
                .expect("path set should succeed"),
            2
        );
        assert_eq!(path.get(&tag), vec![NbtTag::Byte(4), NbtTag::Byte(4)]);
    }

    #[test]
    fn set_updates_indexed_array_elements() {
        let path = parse_nbt_path("bytes[1]").expect("path parses");
        let mut tag = compound([("bytes", NbtTag::ByteArray(vec![1, 2, 3]))]);

        assert_eq!(
            path.set(&mut tag, NbtTag::Byte(9))
                .expect("path set should succeed"),
            1
        );
        assert_eq!(path.get(&tag), vec![NbtTag::Byte(9)]);
    }

    #[test]
    fn set_coerces_numeric_array_values_like_vanilla() {
        let mut tag = compound([
            ("bytes", NbtTag::ByteArray(vec![0])),
            ("ints", NbtTag::IntArray(vec![0, 0])),
            ("longs", NbtTag::LongArray(vec![0])),
        ]);

        assert_eq!(
            parse_nbt_path("bytes[0]")
                .expect("path parses")
                .set(&mut tag, NbtTag::Short(258))
                .expect("path set should succeed"),
            1
        );
        assert_eq!(
            parse_nbt_path("ints[]")
                .expect("path parses")
                .set(&mut tag, NbtTag::Float(3.9))
                .expect("path set should succeed"),
            2
        );
        assert_eq!(
            parse_nbt_path("longs[0]")
                .expect("path parses")
                .set(&mut tag, NbtTag::Double(-1.2))
                .expect("path set should succeed"),
            1
        );

        assert_eq!(
            parse_nbt_path("bytes[0]").expect("path parses").get(&tag),
            vec![NbtTag::Byte(2)]
        );
        assert_eq!(
            parse_nbt_path("ints[]").expect("path parses").get(&tag),
            vec![NbtTag::Int(3), NbtTag::Int(3)]
        );
        assert_eq!(
            parse_nbt_path("longs[0]").expect("path parses").get(&tag),
            vec![NbtTag::Long(-2)]
        );
    }

    #[test]
    fn empty_array_wildcard_reports_vanilla_change_count_for_rejected_value() {
        let path = parse_nbt_path("bytes[]").expect("path parses");
        let mut tag = compound([("bytes", NbtTag::ByteArray(vec![]))]);

        assert_eq!(
            path.set(&mut tag, NbtTag::String("not numeric".into()))
                .expect("path set should succeed"),
            1
        );
        assert_eq!(path.get(&tag), Vec::<NbtTag>::new());
    }

    #[test]
    fn set_regular_list_elements_can_change_type_like_vanilla() {
        let path = parse_nbt_path("values[1]").expect("path parses");
        let all_values = parse_nbt_path("values[]").expect("path parses");
        let mut tag = compound([("values", list([NbtTag::Int(1), NbtTag::Int(2)]))]);

        assert_eq!(
            path.set(&mut tag, NbtTag::String("two".into()))
                .expect("path set should succeed"),
            1
        );
        assert_eq!(
            all_values.get(&tag),
            vec![NbtTag::Int(1), NbtTag::String("two".into())]
        );
    }

    #[test]
    fn predicate_parent_creates_match_in_non_compound_list() {
        let path = parse_nbt_path("items[{id:\"minecraft:stone\"}].Count").expect("path parses");
        let mut tag = compound([("items", list([NbtTag::String("existing".into())]))]);

        assert_eq!(
            path.set(&mut tag, NbtTag::Byte(5))
                .expect("path set should succeed"),
            1
        );
        assert_eq!(path.get(&tag), vec![NbtTag::Byte(5)]);
        assert_eq!(
            parse_nbt_path("items[]").expect("path parses").get(&tag),
            vec![
                NbtTag::String("existing".into()),
                compound([
                    ("id", NbtTag::String("minecraft:stone".into())),
                    ("Count", NbtTag::Byte(5)),
                ]),
            ]
        );
    }

    #[test]
    fn set_predicate_parent_creates_matching_compound() {
        let path = parse_nbt_path("items[{id:\"minecraft:stone\"}].Count").expect("path parses");
        let mut tag = compound([("items", list([]))]);

        assert_eq!(
            path.set(&mut tag, NbtTag::Byte(5))
                .expect("path set should succeed"),
            1
        );
        assert_eq!(path.get(&tag), vec![NbtTag::Byte(5)]);
        assert_eq!(
            parse_nbt_path("items[0].id")
                .expect("id path parses")
                .get(&tag),
            vec![NbtTag::String("minecraft:stone".into())]
        );
    }

    #[test]
    fn set_reports_missing_intermediate_path() {
        let path = parse_nbt_path("items[0].Count").expect("path parses");
        let mut tag = compound([("items", list([]))]);

        assert_eq!(
            path.set(&mut tag, NbtTag::Byte(1)),
            Err(NbtPathMutationError::NothingFound("items[0]".to_owned()))
        );
    }

    #[test]
    fn try_get_names_the_first_node_that_matched_nothing() {
        let path = parse_nbt_path("a.b.c").expect("path parses");
        let tag = compound([("a", compound([]))]);

        assert_eq!(
            path.try_get(&tag),
            Err(NbtPathMutationError::NothingFound("a.b".to_owned()))
        );
    }

    #[test]
    fn remove_deletes_every_selected_tag_and_counts_them() {
        let mut tag = compound([(
            "items",
            list([
                compound([("id", NbtTag::Int(1)), ("keep", NbtTag::Int(0))]),
                compound([("id", NbtTag::Int(2))]),
            ]),
        )]);

        let wildcard = parse_nbt_path("items[].id").expect("path parses");
        assert_eq!(wildcard.remove(&mut tag), 2);
        let missing = parse_nbt_path("items[].id").expect("path parses");
        assert_eq!(missing.remove(&mut tag), 0);

        let by_predicate = parse_nbt_path("items[{keep:0}]").expect("path parses");
        assert_eq!(by_predicate.remove(&mut tag), 1);
        let remaining = parse_nbt_path("items[]").expect("path parses");
        assert_eq!(remaining.count_matching(&tag), 1);
    }

    #[test]
    fn insert_counts_lists_and_handles_negative_indices() {
        let path = parse_nbt_path("items").expect("path parses");
        let mut tag = compound([("items", list([NbtTag::Int(1), NbtTag::Int(2)]))]);

        // -1 appends, 0 prepends, 1 lands between.
        assert_eq!(path.insert(-1, &mut tag, &[NbtTag::Int(9)]), Ok(1));
        assert_eq!(path.insert(0, &mut tag, &[NbtTag::Int(7)]), Ok(1));
        assert_eq!(path.insert(1, &mut tag, &[NbtTag::Int(8)]), Ok(1));
        assert_eq!(
            path.get(&tag),
            vec![list([
                NbtTag::Int(7),
                NbtTag::Int(8),
                NbtTag::Int(1),
                NbtTag::Int(2),
                NbtTag::Int(9),
            ])]
        );
    }

    #[test]
    fn insert_creates_a_missing_list() {
        let path = parse_nbt_path("a.items").expect("path parses");
        let mut tag = compound([]);

        assert_eq!(path.insert(-1, &mut tag, &[NbtTag::Int(1)]), Ok(1));
        assert_eq!(path.get(&tag), vec![list([NbtTag::Int(1)])]);
    }

    #[test]
    fn insert_rejects_mismatched_element_types_without_failing() {
        let path = parse_nbt_path("items").expect("path parses");
        let mut tag = compound([("items", list([NbtTag::Int(1)]))]);

        // `ListTag.addTag` answers `false`, so nothing changed and no error.
        assert_eq!(
            path.insert(-1, &mut tag, &[NbtTag::String("x".into())]),
            Ok(0)
        );
    }

    #[test]
    fn insert_reports_bad_indices_and_non_lists() {
        let path = parse_nbt_path("items").expect("path parses");
        let mut tag = compound([("items", list([NbtTag::Int(1)]))]);
        assert_eq!(
            path.insert(5, &mut tag, &[NbtTag::Int(2)]),
            Err(NbtPathMutationError::InvalidIndex(5))
        );

        let mut scalar = compound([("items", NbtTag::Int(1))]);
        assert!(matches!(
            path.insert(0, &mut scalar, &[NbtTag::Int(2)]),
            Err(NbtPathMutationError::ExpectedList(_))
        ));
    }

    #[test]
    fn insert_coerces_into_numeric_arrays() {
        let path = parse_nbt_path("ids").expect("path parses");
        let mut tag = compound([("ids", NbtTag::IntArray(vec![1, 2]))]);

        assert_eq!(path.insert(-1, &mut tag, &[NbtTag::Byte(3)]), Ok(1));
        assert_eq!(path.get(&tag), vec![NbtTag::IntArray(vec![1, 2, 3])]);
        assert_eq!(
            path.insert(0, &mut tag, &[NbtTag::String("x".into())]),
            Ok(0)
        );
    }

    #[test]
    fn merge_compound_counts_only_compounds_that_changed() {
        let path = parse_nbt_path("items[]").expect("path parses");
        let mut tag = compound([(
            "items",
            list([
                compound([("a", NbtTag::Int(1))]),
                compound([("a", NbtTag::Int(2))]),
            ]),
        )]);
        let mut source = NbtCompound::new();
        source.insert("a", NbtTag::Int(1));

        assert_eq!(path.merge_compound(&mut tag, &source), Ok(1));
        assert_eq!(path.merge_compound(&mut tag, &source), Ok(0));
    }

    #[test]
    fn merge_compound_rejects_non_compound_targets() {
        let path = parse_nbt_path("value").expect("path parses");
        let mut tag = compound([("value", NbtTag::Int(1))]);

        assert!(matches!(
            path.merge_compound(&mut tag, &NbtCompound::new()),
            Err(NbtPathMutationError::ExpectedObject(_))
        ));
    }
}
