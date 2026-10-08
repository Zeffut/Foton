//! Human-readable summary of an import.

use std::collections::BTreeMap;
use std::fmt::{self, Display, Formatter};

use super::{DimensionReport, ImportReport};

/// Writes `label: name x count, ...` for a non-empty tally.
fn write_tally(
    formatter: &mut Formatter<'_>,
    label: &str,
    tally: &BTreeMap<String, u64>,
) -> fmt::Result {
    if tally.is_empty() {
        return Ok(());
    }
    let entries: Vec<String> = tally
        .iter()
        .map(|(name, count)| format!("{name} x{count}"))
        .collect();
    writeln!(formatter, "    {label}: {}", entries.join(", "))
}

impl Display for DimensionReport {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        writeln!(formatter, "{} -> {}", self.dimension, self.world_key)?;
        writeln!(
            formatter,
            "    {} regions, {} chunks found, {} imported",
            self.regions, self.chunks_found, self.imported
        )?;
        writeln!(
            formatter,
            "    {} block entities, {} entities",
            self.block_entities, self.entities
        )?;
        let not_full: u64 = self.not_full.values().sum();
        if not_full > 0 {
            writeln!(
                formatter,
                "    {not_full} chunks vanilla never finished generating were left out, so Foton generates them"
            )?;
        }
        if self.unreadable > 0 {
            writeln!(
                formatter,
                "    {} chunks could not be read and were left out:",
                self.unreadable
            )?;
        }
        for example in &self.unreadable_examples {
            writeln!(formatter, "      {example}")?;
        }
        if self.outdated_chunks > 0 {
            writeln!(
                formatter,
                "    {} chunks still at an older DataVersion were left out (--skip-outdated), so Foton generates them",
                self.outdated_chunks
            )?;
        }
        if self.outdated_entity_chunks > 0 {
            writeln!(
                formatter,
                "    {} entity chunks still at an older DataVersion were left out; those chunks were imported without entities",
                self.outdated_entity_chunks
            )?;
        }
        if self.unreadable_entity_chunks > 0 {
            writeln!(
                formatter,
                "    {} entity chunks could not be read; those chunks were imported without entities",
                self.unreadable_entity_chunks
            )?;
        }
        let issues = &self.issues;
        write_tally(
            formatter,
            "unknown blocks, replaced by air",
            &issues.unknown_blocks,
        )?;
        write_tally(
            formatter,
            "blocks with properties Foton rejects (property ignored)",
            &issues.rejected_block_properties,
        )?;
        write_tally(
            formatter,
            "unknown biomes, replaced by plains",
            &issues.unknown_biomes,
        )?;
        write_tally(
            formatter,
            "unknown block entities, dropped",
            &issues.unknown_block_entities,
        )?;
        write_tally(
            formatter,
            "block entities on a block that does not take them, dropped",
            &issues.misplaced_block_entities,
        )?;
        write_tally(
            formatter,
            "unknown entities, dropped",
            &issues.unknown_entities,
        )?;
        if issues.malformed_entities > 0 {
            writeln!(
                formatter,
                "    {} entities without a position or id, dropped",
                issues.malformed_entities
            )?;
        }
        if issues.chunks_without_light > 0 {
            writeln!(
                formatter,
                "    {} chunks had no light data in the source; they will look dark until lit",
                issues.chunks_without_light
            )?;
        }
        Ok(())
    }
}

impl Display for ImportReport {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        for dimension in &self.dimensions {
            write!(formatter, "{dimension}")?;
        }
        match (self.level_written, self.seed) {
            (true, Some(seed)) => writeln!(formatter, "level data written, seed {seed}")?,
            _ => writeln!(
                formatter,
                "no level.dat found and no --seed given: level.toml was not written, so Foton \
                 will pick its own seed and generate new terrain that does not match the old world"
            )?,
        }
        writeln!(formatter, "done in {:.1?}", self.elapsed)
    }
}
