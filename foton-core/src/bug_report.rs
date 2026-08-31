//! Bug reports filed by players from inside the game.
//!
//! A test session's whole value is what the testers noticed, and the gap
//! between noticing and reporting is where most of it is lost. So the report
//! is filed where the player is standing, and everything that can be captured
//! without asking -- who, where, which world, which build -- is captured
//! rather than typed.
//!
//! Reports are appended to `reports/bugs.jsonl`, one JSON object per line.
//! Append-only and line-delimited on purpose: concurrent writers cannot
//! corrupt each other's records the way a rewritten array would, a truncated
//! last line costs one report rather than the file, and every ordinary tool
//! reads it.

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Error as IoError, Result as IoResult, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use reqwest::header::CONTENT_TYPE;
use serde::{Deserialize, Serialize};
use tokio::time::{Duration, sleep};

use crate::config::BugReportWebhook;

/// Where reports are kept, relative to the server's working directory.
const REPORTS_DIR: &str = "reports";

/// The file reports are appended to.
const REPORTS_FILE: &str = "bugs.jsonl";

/// The longest description a single report may carry.
///
/// Long enough for real reproduction steps, short enough that one player
/// cannot fill the disk.
pub const MAX_DESCRIPTION: usize = 4096;

/// What part of the game a report is about.
///
/// A free-text category would be unsortable within a day, so the list is
/// closed. `Other` is deliberate: a tester who cannot place their bug should
/// still be able to file it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BugCategory {
    /// Placing, breaking, or a block's own behavior.
    Blocks,
    /// What an item does in hand, or its components.
    Items,
    /// Spawning, AI, pathing, or a mob's own behavior.
    Mobs,
    /// Damage, knockback, shields, enchantment effects.
    Combat,
    /// Signals, timing, and the blocks that carry them.
    Redstone,
    /// Terrain, biomes, structures, and their contents.
    Worldgen,
    /// A command's parsing, permissions, or result.
    Commands,
    /// Containers, menus, crafting, and item movement.
    Inventory,
    /// A sound that is missing, wrong, or misplaced.
    Sounds,
    /// Lag spikes, stutter, and anything that got slow.
    Performance,
    /// Anything that fits nowhere else; never reject a report for want of a category.
    Other,
}

impl BugCategory {
    /// Every category, in the order a form should offer them.
    pub const ALL: [Self; 11] = [
        Self::Blocks,
        Self::Items,
        Self::Mobs,
        Self::Combat,
        Self::Redstone,
        Self::Worldgen,
        Self::Commands,
        Self::Inventory,
        Self::Sounds,
        Self::Performance,
        Self::Other,
    ];

    /// The name this category is written and parsed as.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Blocks => "blocks",
            Self::Items => "items",
            Self::Mobs => "mobs",
            Self::Combat => "combat",
            Self::Redstone => "redstone",
            Self::Worldgen => "worldgen",
            Self::Commands => "commands",
            Self::Inventory => "inventory",
            Self::Sounds => "sounds",
            Self::Performance => "performance",
            Self::Other => "other",
        }
    }

    /// The name a player reads in the report form.
    ///
    /// Separate from [`Self::name`] on purpose: the written name is a stable
    /// key the report file is sorted by, and must not move when the wording
    /// shown to a tester is improved.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Blocks => "Blocks",
            Self::Items => "Items",
            Self::Mobs => "Mobs and AI",
            Self::Combat => "Combat and damage",
            Self::Redstone => "Redstone",
            Self::Worldgen => "World generation",
            Self::Commands => "Commands",
            Self::Inventory => "Inventory and menus",
            Self::Sounds => "Sounds",
            Self::Performance => "Lag and performance",
            Self::Other => "Something else",
        }
    }

    /// Parses a category name, case-insensitively.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        let lowered = raw.to_ascii_lowercase();
        Self::ALL.into_iter().find(|c| c.name() == lowered)
    }
}

/// One report, as it is written to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BugReport {
    /// Seconds since the Unix epoch, so the file sorts by time on its own.
    pub at: u64,
    /// The reporter's name at the time of filing.
    pub player: String,
    /// The reporter's UUID, which their name is not a substitute for.
    pub uuid: String,
    /// The world the report was filed in.
    pub world: String,
    /// Where the reporter was standing, rounded to a sensible precision.
    pub position: [f64; 3],
    /// What part of the game the report is about.
    pub category: BugCategory,
    /// What the reporter typed: what happened and how to see it again.
    pub description: String,
    /// The build the report was filed against.
    ///
    /// Without it a fixed bug and a live one look identical in the file.
    pub version: String,
}

impl BugReport {
    /// Appends this report and returns its one-based number in the file.
    ///
    /// # Errors
    ///
    /// Returns the underlying I/O error if the directory cannot be created or
    /// the file cannot be appended to.
    pub fn append_in(&self, run_dir: &Path) -> IoResult<usize> {
        let dir = run_dir.join(REPORTS_DIR);
        fs::create_dir_all(&dir)?;
        let path = dir.join(REPORTS_FILE);

        let mut line = serde_json::to_string(self).map_err(IoError::other)?;
        line.push('\n');

        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
        file.write_all(line.as_bytes())?;
        file.flush()?;

        Ok(count_lines(&path).unwrap_or(0))
    }

    /// Reads every report on file, oldest first.
    ///
    /// A line that does not parse is skipped rather than fatal: one bad record
    /// must not hide the rest.
    ///
    /// # Errors
    ///
    /// Returns the underlying I/O error if the file exists but cannot be read.
    pub fn read_all(run_dir: &Path) -> IoResult<Vec<Self>> {
        let path = reports_path(run_dir);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let file = fs::File::open(&path)?;
        Ok(BufReader::new(file)
            .lines()
            .map_while(Result::ok)
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str(&line).ok())
            .collect())
    }

    /// Builds a report stamped with the current time.
    #[must_use]
    pub fn now(
        player: String,
        uuid: String,
        world: String,
        position: [f64; 3],
        category: BugCategory,
        description: String,
    ) -> Self {
        Self {
            at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |since| since.as_secs()),
            player,
            uuid,
            world,
            position: position.map(|value| (value * 100.0).round() / 100.0),
            category,
            description,
            version: env!("CARGO_PKG_VERSION").to_owned(),
        }
    }
}

/// The path reports live at for a given server directory.
#[must_use]
pub fn reports_path(run_dir: &Path) -> PathBuf {
    run_dir.join(REPORTS_DIR).join(REPORTS_FILE)
}

fn count_lines(path: &Path) -> IoResult<usize> {
    let file = fs::File::open(path)?;
    Ok(BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter(|line| !line.trim().is_empty())
        .count())
}

/// How many times one report is offered to the webhook before it is left behind.
const FORWARD_ATTEMPTS: usize = 3;

/// How long a single delivery may take.
const FORWARD_TIMEOUT: Duration = Duration::from_secs(5);

/// How long to wait before offering a report again.
const FORWARD_RETRY_DELAY: Duration = Duration::from_secs(2);

/// One report as the endpoint receives it: the record plus its file number.
///
/// The number is what makes a delivery identifiable. Without it the receiver
/// cannot tell a retry from a second report with the same text, and a
/// backfill cannot know where it left off.
#[derive(Serialize)]
struct ForwardedReport<'report> {
    number: usize,
    #[serde(flatten)]
    report: &'report BugReport,
}

/// Sends a freshly filed report to the configured endpoint, in the background.
///
/// Deliberately fire-and-forget: the report is already on disk before this is
/// called, so a receiver that is down or slow costs the site some freshness
/// and costs the reporter nothing. The final failure names the report's number
/// so a backfill knows exactly what to replay out of `bugs.jsonl`.
pub fn forward(webhook: &BugReportWebhook, report: &BugReport, number: usize) {
    let Ok(body) = serde_json::to_vec(&ForwardedReport { number, report }) else {
        log::error!("bug report #{number} could not be encoded for forwarding");
        return;
    };
    let url = webhook.url.clone();
    let token = webhook.token.clone();

    drop(tokio::spawn(async move {
        let client = reqwest::Client::new();
        for attempt in 1..=FORWARD_ATTEMPTS {
            let mut request = client
                .post(url.clone())
                .header(CONTENT_TYPE, "application/json")
                .timeout(FORWARD_TIMEOUT)
                .body(body.clone());
            if let Some(token) = &token {
                request = request.bearer_auth(token);
            }

            match request.send().await {
                Ok(response) if response.status().is_success() => return,
                Ok(response) => {
                    let status = response.status();
                    // A refusal the receiver meant is not worth two more
                    // rounds; only a server-side fault is likely to pass later.
                    if !status.is_server_error() {
                        log::warn!(
                            "bug report #{number} was refused with {status}; it stays in \
                             bugs.jsonl and can be backfilled"
                        );
                        return;
                    }
                    log::debug!("bug report #{number} attempt {attempt} got {status}");
                }
                Err(error) => {
                    log::debug!("bug report #{number} attempt {attempt} failed: {error}");
                }
            }

            if attempt < FORWARD_ATTEMPTS {
                sleep(FORWARD_RETRY_DELAY).await;
            }
        }

        log::warn!(
            "bug report #{number} was not delivered after {FORWARD_ATTEMPTS} attempts; it is \
             on disk and the site is behind by at least this one"
        );
    }));
}

#[cfg(test)]
mod tests {
    use std::env::temp_dir;
    use std::process::id as process_id;

    use super::*;

    /// The payload is the record itself with its file number beside it.
    ///
    /// This is the contract whatever receives these reads, so it is pinned
    /// rather than left to whatever `serde` happens to do. The number is what
    /// makes a delivery identifiable: without it a receiver cannot tell a
    /// retry from a second report with the same text, and a backfill has no
    /// way to know where it left off.
    #[test]
    fn a_forwarded_report_carries_its_number_beside_the_record() {
        let report = sample("the pig went through a wall");

        let payload = serde_json::to_value(ForwardedReport {
            number: 7,
            report: &report,
        })
        .expect("a report encodes");
        let object = payload.as_object().expect("the payload is one flat object");

        assert_eq!(
            object.get("number").and_then(serde_json::Value::as_u64),
            Some(7),
            "the number identifies the delivery"
        );
        for field in [
            "at",
            "player",
            "uuid",
            "world",
            "position",
            "category",
            "description",
            "version",
        ] {
            assert!(
                object.contains_key(field),
                "the record's `{field}` has to survive forwarding; the receiver has \
                 nothing else to work from"
            );
        }
        assert_eq!(
            object.get("player").and_then(serde_json::Value::as_str),
            Some("Tester"),
            "the record is flattened beside the number, not nested under a key"
        );
    }

    fn sample(description: &str) -> BugReport {
        BugReport::now(
            "Tester".to_owned(),
            "00000000-0000-0000-0000-000000000001".to_owned(),
            "minecraft:overworld".to_owned(),
            [1.234_567, 64.0, -3.0],
            BugCategory::Mobs,
            description.to_owned(),
        )
    }

    /// Two reports filed in a row are both kept, and numbered as they land.
    ///
    /// The obvious wrong implementation -- serialize the whole list and
    /// rewrite the file -- passes a single-report test and silently drops
    /// every report but the last as soon as two testers file at once.
    #[test]
    fn reports_accumulate_rather_than_replace_each_other() {
        let dir = temp_dir().join(format!("foton-bugs-{}", process_id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");

        assert_eq!(sample("first").append_in(&dir).expect("append"), 1);
        assert_eq!(sample("second").append_in(&dir).expect("append"), 2);

        let all = BugReport::read_all(&dir).expect("read");
        assert_eq!(all.len(), 2, "both reports have to survive");
        assert_eq!(all[0].description, "first");
        assert_eq!(all[1].description, "second");
        assert!(
            (all[0].position[0] - 1.23).abs() < 1.0e-9,
            "coordinates keep two decimals: enough to walk back to the spot,              short enough to read in a listing"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// A line that cannot be parsed costs its own record and nothing else.
    #[test]
    fn one_corrupt_line_does_not_hide_the_others() {
        let dir = temp_dir().join(format!("foton-bugs-bad-{}", process_id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");

        sample("good").append_in(&dir).expect("append");
        let path = reports_path(&dir);
        let mut file = OpenOptions::new().append(true).open(&path).expect("open");
        file.write_all(b"{ this is not json\n").expect("write");
        sample("also good").append_in(&dir).expect("append");

        let all = BugReport::read_all(&dir).expect("read");
        assert_eq!(all.len(), 2, "the two valid records still come back");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn categories_round_trip_through_their_names() {
        for category in BugCategory::ALL {
            assert_eq!(BugCategory::parse(category.name()), Some(category));
        }
        assert_eq!(BugCategory::parse("MOBS"), Some(BugCategory::Mobs));
        assert_eq!(BugCategory::parse("nonsense"), None);
    }
}
