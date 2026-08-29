use super::*;

impl StructureTemplate {
    pub(crate) fn load_vanilla(registry: &Registry, key: &Identifier) -> Result<Self, String> {
        let Some(bytes) = vanilla_template_pools::vanilla_template_nbt_bytes(key) else {
            return Err(format!("vanilla structure template {key} is not bundled"));
        };
        Self::load_gzip_nbt(registry, bytes, &key.to_string())
    }

    pub(super) fn load_gzip_nbt(
        registry: &Registry,
        bytes: &[u8],
        context: &str,
    ) -> Result<Self, String> {
        let mut decoder = GzDecoder::new(bytes);
        let mut data = Vec::new();
        decoder
            .read_to_end(&mut data)
            .map_err(|err| format!("failed to decompress structure template {context}: {err}"))?;

        let nbt = read_nbt(&mut Cursor::new(&data))
            .map_err(|err| format!("failed to parse structure template {context}: {err}"))?;
        let root = match nbt {
            BorrowedNbt::Some(root) => root,
            BorrowedNbt::None => {
                return Err(format!("structure template {context} is empty"));
            }
        };
        let compound = root.as_compound();

        let size = Self::read_vec3(compound.list("size"), context, "size")?;
        let palettes = Self::read_palettes(registry, &compound, context)?;
        let blocks = compound
            .list("blocks")
            .and_then(|list| list.compounds())
            .ok_or_else(|| format!("structure template {context} has non-compound blocks list"))?;

        let mut loaded_palettes = Vec::with_capacity(palettes.len());
        for palette in &palettes {
            loaded_palettes.push(StructureTemplatePalette {
                blocks: Self::read_blocks(registry, &blocks, palette, context)?,
            });
        }

        let entities = Self::read_entities(&compound, context)?;
        let author = compound
            .string("author")
            .map(|author| author.to_str().into_owned())
            .unwrap_or_default();

        Ok(Self {
            author,
            size,
            palettes: loaded_palettes,
            entities,
        })
    }

    pub(super) fn read_vec3(
        list: Option<BorrowedNbtList<'_, '_>>,
        context: &str,
        field: &str,
    ) -> Result<IVec3, String> {
        let ints = list
            .and_then(|list| list.ints())
            .ok_or_else(|| format!("structure template {context} has non-int {field} list"))?;
        if ints.len() < 3 {
            return Err(format!(
                "structure template {context} {field} list has fewer than 3 entries"
            ));
        }
        Ok(IVec3::new(ints[0], ints[1], ints[2]))
    }

    pub(super) fn read_vec3d(
        list: Option<BorrowedNbtList<'_, '_>>,
        context: &str,
        field: &str,
    ) -> Result<DVec3, String> {
        let doubles = list
            .and_then(|list| list.doubles())
            .ok_or_else(|| format!("structure template {context} has non-double {field} list"))?;
        if doubles.len() < 3 {
            return Err(format!(
                "structure template {context} {field} list has fewer than 3 entries"
            ));
        }
        Ok(DVec3::new(doubles[0], doubles[1], doubles[2]))
    }

    pub(super) fn read_palettes(
        registry: &Registry,
        compound: &BorrowedNbtCompound<'_, '_>,
        context: &str,
    ) -> Result<Vec<Vec<BlockStateId>>, String> {
        if let Some(palette) = compound.list("palette").and_then(|list| list.compounds()) {
            return Ok(vec![Self::read_palette(registry, &palette, context)?]);
        }

        let palettes = compound
            .list("palettes")
            .and_then(|list| list.lists())
            .ok_or_else(|| {
                format!("structure template {context} is missing palette or palettes")
            })?;
        if palettes.is_empty() {
            return Err(format!(
                "structure template {context} has empty palettes list"
            ));
        }

        let mut result = Vec::with_capacity(palettes.len());
        for palette in palettes {
            let entries = palette.compounds().ok_or_else(|| {
                format!("structure template {context} has non-compound palette entry")
            })?;
            result.push(Self::read_palette(registry, &entries, context)?);
        }
        Ok(result)
    }

    pub(super) fn read_palette(
        registry: &Registry,
        entries: &BorrowedNbtCompoundList<'_, '_>,
        context: &str,
    ) -> Result<Vec<BlockStateId>, String> {
        let mut states = Vec::with_capacity(entries.len());
        for entry in entries.clone() {
            let Some(name) = entry.string("Name") else {
                return Err(format!(
                    "structure template {context} has palette entry without Name"
                ));
            };
            let name = Identifier::from_str(name.to_str().as_ref()).map_err(|err| {
                format!("structure template {context} has invalid block identifier: {err}")
            })?;
            let mut properties = BTreeMap::new();
            if let Some(props) = entry.compound("Properties") {
                for (key, value) in props.iter() {
                    let Some(value) = value.string() else {
                        return Err(format!(
                            "structure template {context} has non-string property {} on {name}",
                            key.to_str()
                        ));
                    };
                    properties.insert(key.to_str().into_owned(), value.to_str().into_owned());
                }
            }
            states.push(WorldgenStateResolver::block_state_from_data(
                registry,
                &BlockStateData { name, properties },
                "structure template palette",
            ));
        }
        Ok(states)
    }

    pub(super) fn read_blocks(
        registry: &Registry,
        blocks: &BorrowedNbtCompoundList<'_, '_>,
        palette: &[BlockStateId],
        context: &str,
    ) -> Result<Vec<StructureBlockInfo>, String> {
        let mut full_blocks = Vec::new();
        let mut other_blocks = Vec::new();
        let mut block_entities = Vec::new();

        for block in blocks.clone() {
            let pos = Self::read_vec3(block.list("pos"), context, "block pos")?;
            let state_index = block
                .int("state")
                .ok_or_else(|| format!("structure template {context} block is missing state"))?;
            if state_index < 0 {
                return Err(format!(
                    "structure template {context} has negative palette state {state_index}"
                ));
            }
            let state_index = usize::try_from(state_index).map_err(|_| {
                format!("structure template {context} state index does not fit usize")
            })?;
            let Some(&state) = palette.get(state_index) else {
                return Err(format!(
                    "structure template {context} state index {state_index} exceeds palette length {}",
                    palette.len()
                ));
            };
            let nbt = block.compound("nbt").map(|nbt| nbt.to_owned());
            let info = StructureBlockInfo {
                pos: BlockPos::new(pos[0], pos[1], pos[2]),
                state,
                nbt,
            };

            if info.nbt.is_some() {
                block_entities.push(info);
            } else if Self::is_static_full_block(registry, state) {
                full_blocks.push(info);
            } else {
                other_blocks.push(info);
            }
        }

        Self::sort_block_infos(&mut full_blocks);
        Self::sort_block_infos(&mut other_blocks);
        Self::sort_block_infos(&mut block_entities);

        full_blocks.extend(other_blocks);
        full_blocks.extend(block_entities);
        Ok(full_blocks)
    }

    pub(super) fn read_entities(
        compound: &BorrowedNbtCompound<'_, '_>,
        context: &str,
    ) -> Result<Vec<StructureEntityInfo>, String> {
        let Some(entities) = compound.list("entities").and_then(|list| list.compounds()) else {
            return Ok(Vec::new());
        };

        let mut result = Vec::with_capacity(entities.len());
        for entity in entities.clone() {
            let pos = Self::read_vec3d(entity.list("pos"), context, "entity pos")?;
            let block_pos = Self::read_vec3(entity.list("blockPos"), context, "entity blockPos")?;
            let entity_nbt = entity.compound("nbt").ok_or_else(|| {
                format!("structure template {context} has entity entry without nbt")
            })?;
            // A template's entities are ordinary vanilla save compounds, so the
            // same reader `/summon` uses decodes them. `Pos` and `UUID` are the
            // two fields a template does not take from the compound: the
            // position is the template-relative `pos` above, and each placement
            // mints a fresh UUID so two copies of one structure are two
            // entities.
            let loaded = read_entity_nbt(&entity_nbt).ok_or_else(|| {
                let id = entity_nbt
                    .string("id")
                    .map_or_else(|| "<missing>".to_owned(), |id| id.to_str().into_owned());
                format!("structure template {context} references unknown entity type {id}")
            })?;
            result.push(StructureEntityInfo {
                pos,
                block_pos: BlockPos::new(block_pos[0], block_pos[1], block_pos[2]),
                entity_type: loaded.entity_type,
                rotation: loaded.rotation,
                velocity: loaded.velocity,
                fall_distance: loaded.fall_distance,
                fire_freeze: loaded.fire_freeze,
                on_ground: loaded.on_ground,
                save_data: loaded.save_data,
                nbt: loaded.remainder,
            });
        }

        Ok(result)
    }

    pub(super) fn is_static_full_block(registry: &Registry, state: BlockStateId) -> bool {
        let Some(block) = registry.blocks.by_state_id(state) else {
            return false;
        };
        !block.config.dynamic_shape
            && blocks::shapes::is_shape_full_block(
                registry.blocks.get_static_collision_shape(state),
            )
    }

    pub(super) fn sort_block_infos(blocks: &mut [StructureBlockInfo]) {
        blocks.sort_by(|left, right| {
            left.pos
                .y()
                .cmp(&right.pos.y())
                .then(left.pos.x().cmp(&right.pos.x()))
                .then(left.pos.z().cmp(&right.pos.z()))
        });
    }

    /// Returns who saved this template, or the empty string.
    ///
    /// Vanilla parity: `StructureTemplate.getAuthor`.
    pub(crate) fn author(&self) -> &str {
        &self.author
    }

    pub(crate) const fn size(&self, rotation: Rotation) -> IVec3 {
        rotation.rotate_size(self.size)
    }

    pub(crate) const fn zero_position_with_transform(
        &self,
        zero_pos: BlockPos,
        rotation: Rotation,
    ) -> BlockPos {
        let x = self.size.x - 1;
        let z = self.size.z - 1;
        match rotation {
            Rotation::None => zero_pos,
            Rotation::Clockwise90 => zero_pos.offset(z, 0, 0),
            Rotation::Clockwise180 => zero_pos.offset(x, 0, z),
            Rotation::CounterClockwise90 => zero_pos.offset(0, 0, x),
        }
    }
}
