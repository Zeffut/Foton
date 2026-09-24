use crate::config::{LogConfig, LogTimeFormat};
use chrono::Utc;
use crossterm::{
    style::{Color::DarkGrey, ResetColor, SetForegroundColor},
    terminal::{self, Clear, ClearType, disable_raw_mode},
};
use foton_utils::locks::AsyncRwLock;
use foton_utils::logger::{FOTON_LOGGER, FotonLogger, Level, LogData};
use std::{
    io::Write,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{self, Instant},
};
use tokio::{sync::mpsc, task, time::timeout};
use tokio_util::sync::CancellationToken;
use tracing::Subscriber;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;

mod file;
mod history;
mod input;
mod output;
mod selection;
mod state;
mod suggestions;

/// Logging must never become an attacker-controlled unbounded allocation.
const LOG_QUEUE_CAPACITY: usize = 8_192;
const LOG_PRIORITY_QUEUE_CAPACITY: usize = 256;
const LOG_SHUTDOWN_DRAIN_LIMIT: usize = 256;

/// Returns the terminal width, falling back to 80 columns if unavailable or it's <= 0.
fn terminal_width() -> usize {
    terminal::size().map_or(80, |(w, _)| if w == 0 { 80 } else { w as usize })
}
/// Returns the terminal height, falling back to 30 rows if unavailable or 1 if it's <= 0.
fn terminal_height() -> usize {
    terminal::size().map_or(30, |(_, h)| if h == 0 { 30 } else { h as usize })
}

pub(crate) use state::LogState;

pub(crate) enum Move {
    None,
    Up,
    Down,
}

/// A logger implementation with commands suggestions
pub struct CommandLogger {
    input: Arc<AsyncRwLock<LogState>>,
    sender: mpsc::Sender<(Level, LogData)>,
    priority_sender: mpsc::Sender<(Level, LogData)>,
    dropped_entries: AtomicU64,
    dropped_priority_entries: AtomicU64,
    cancel_token: CancellationToken,
    stopped: CancellationToken,
    log_stopped: CancellationToken,
    start_time: Instant,
    log_config: Option<LogConfig>,
}

impl CommandLogger {
    /// Initializes the `CommandLogger`
    pub async fn init(
        cancel_token: CancellationToken,
        log_config: Option<LogConfig>,
    ) -> Result<Arc<Self>, String> {
        let (sender, receiver) = mpsc::channel(LOG_QUEUE_CAPACITY);
        let (priority_sender, priority_receiver) = mpsc::channel(LOG_PRIORITY_QUEUE_CAPACITY);
        let log_cancel_token = CancellationToken::new();
        let input = LogState::new(log_config.as_ref(), cancel_token)
            .await
            .map_err(|err| format!("failed to initialize logger state: {err}"))?;

        let log = Arc::new(Self {
            input: Arc::new(AsyncRwLock::const_new(input)),
            sender,
            priority_sender,
            dropped_entries: AtomicU64::new(0),
            dropped_priority_entries: AtomicU64::new(0),
            cancel_token: log_cancel_token.clone(),
            stopped: CancellationToken::new(),
            log_stopped: CancellationToken::new(),
            start_time: Instant::now(),
            log_config,
        });
        task::spawn(log.clone().log_loop(receiver, priority_receiver));
        task::spawn(log.clone().input_main());
        FOTON_LOGGER
            .set(log.clone())
            .map_err(|_| "Foton logger is already initialized".to_string())?;
        Ok(log)
    }

    /// Stops the logger and waits for cleanup to complete
    pub async fn stop(&self) {
        self.cancel_token.cancel();
        if timeout(time::Duration::from_secs(1), self.stopped.cancelled())
            .await
            .is_err()
        {
            let _ = disable_raw_mode();
            self.stopped.cancel();
        }
        if timeout(time::Duration::from_secs(1), self.log_stopped.cancelled())
            .await
            .is_err()
        {
            eprintln!("Timed out waiting for logger to flush pending entries");
        }
    }

    async fn log_loop(
        self: Arc<Self>,
        mut receiver: mpsc::Receiver<(Level, LogData)>,
        mut priority_receiver: mpsc::Receiver<(Level, LogData)>,
    ) {
        loop {
            tokio::select! {
                biased;
                () = self.cancel_token.cancelled() => {
                    let (pending, abandoned_priority, abandoned) = take_shutdown_batch(
                        &mut priority_receiver,
                        &mut receiver,
                        LOG_SHUTDOWN_DRAIN_LIMIT,
                    );
                    self.dropped_priority_entries
                        .fetch_add(abandoned_priority, Ordering::Relaxed);
                    self.dropped_entries.fetch_add(abandoned, Ordering::Relaxed);
                    for (lvl, data) in pending {
                        self.write_entry(lvl, data).await;
                    }
                    self.report_dropped_entries();
                    self.flush_file().await;
                    self.log_stopped.cancel();
                    break;
                }
                Some((lvl, data)) = priority_receiver.recv() => {
                    self.report_dropped_entries();
                    self.write_entry(lvl, data).await;
                }
                Some((lvl, data)) = receiver.recv() => {
                    self.report_dropped_entries();
                    self.write_entry(lvl, data).await;
                }
            }
        }
    }

    fn report_dropped_entries(&self) {
        let dropped_priority = self.dropped_priority_entries.swap(0, Ordering::Relaxed);
        if dropped_priority != 0 {
            eprintln!(
                "Foton priority logger queue full; dropped {dropped_priority} WARN/ERROR entries"
            );
        }
        let dropped = self.dropped_entries.swap(0, Ordering::Relaxed);
        if dropped != 0 {
            eprintln!("Foton logger queue full; dropped {dropped} log entries");
        }
    }

    async fn write_entry(&self, lvl: Level, data: LogData) {
        let (lvl, data) = self.write_log_entry(lvl, data).await;
        if self.log_config.as_ref().is_some_and(|l| l.log_file) {
            self.write_file_entry(lvl, data).await;
        }
    }

    async fn write_log_entry(&self, lvl: Level, data: LogData) -> (Level, LogData) {
        let mut input = self.input.write().await;

        if let Err(err) = input.out.cursor_to(0) {
            log::error!("{err}");
            return (lvl, data);
        }

        let time_str = self.format_time();
        let module_path_str = self.format_module_path(&data, true);
        let extra_str = self.format_extra(&lvl, &data, true);
        let rendered = normalize_terminal_newlines(&format!(
            "{time_str}{lvl} {module_path_str}{}{extra_str}",
            data.message
        ));
        let consumed_rows = rendered_terminal_rows(&rendered, terminal_width());

        if let Err(err) = writeln!(
            input.out,
            "{}{rendered}\r",
            Clear(ClearType::FromCursorDown),
        ) {
            log::error!("{err}");
            return (lvl, data);
        }

        input.completion.consume_reserved_rows(consumed_rows);

        let pos = input.out.pos;
        if let Err(err) = input.out.cursor_to_relative(pos) {
            log::error!("{err}");
        }
        if let Err(err) = input.rewrite_current_input() {
            log::error!("{err}");
        }
        (lvl, data)
    }

    async fn write_file_entry(&self, lvl: Level, data: LogData) {
        let mut input = self.input.write().await;

        let time_str = self.format_time();
        let module_path_str = self.format_module_path(&data, false);
        let extra_str = self.format_extra(&lvl, &data, false);

        if let Err(err) = writeln!(
            input.file,
            "{time_str}{lvl:?} {module_path_str}{}{extra_str}",
            strip_ansi_escapes::strip_str(&data.message),
        ) {
            input.file.disable();
            eprintln!("Failed to write log file; disabling file logging: {err}");
        }
    }

    async fn flush_file(&self) {
        let mut input = self.input.write().await;
        if let Err(err) = input.file.flush() {
            eprintln!("Failed to flush log file: {err}");
        }
    }

    fn format_time(&self) -> String {
        match self.log_config.as_ref().map(|l| &l.time) {
            Some(LogTimeFormat::Date) => {
                let time: chrono::DateTime<Utc> = time::SystemTime::now().into();
                format!("{} ", time.format("%T:%3f"))
            }
            Some(LogTimeFormat::Uptime) => {
                let elapsed = self.start_time.elapsed();
                format!("{:>6.2}s ", elapsed.as_secs_f64())
            }
            _ => String::new(),
        }
    }

    fn format_module_path(&self, data: &LogData, color: bool) -> String {
        if self.log_config.as_ref().is_some_and(|l| l.module_path) {
            if color {
                format!(
                    " {}{}{} ",
                    SetForegroundColor(DarkGrey),
                    data.module_path,
                    ResetColor
                )
            } else {
                format!(" {} ", data.module_path)
            }
        } else {
            String::new()
        }
    }

    fn format_extra(&self, lvl: &Level, data: &LogData, color: bool) -> String {
        if should_render_extra(lvl, self.log_config.as_ref()) {
            if color {
                format!(
                    "{}{}{}",
                    SetForegroundColor(DarkGrey),
                    data.extra,
                    ResetColor
                )
            } else {
                data.extra.clone()
            }
        } else {
            String::new()
        }
    }
}

fn rendered_terminal_rows(rendered: &str, terminal_width: usize) -> usize {
    let terminal_width = terminal_width.max(1);
    let rendered = rendered.replace('\t', "        ");
    strip_ansi_escapes::strip_str(&rendered)
        .split('\n')
        .map(|line| {
            let columns = line
                .bytes()
                .map(|byte| usize::from(byte != b'\r'))
                .sum::<usize>();
            columns.div_ceil(terminal_width).max(1)
        })
        .sum()
}

fn normalize_terminal_newlines(rendered: &str) -> String {
    rendered.replace("\r\n", "\n").replace('\n', "\r\n")
}

impl FotonLogger for CommandLogger {
    fn log(&self, lvl: Level, data: LogData) {
        enqueue_log(
            &self.sender,
            &self.priority_sender,
            &self.dropped_entries,
            &self.dropped_priority_entries,
            (lvl, data),
        );
    }
}

fn enqueue_log(
    sender: &mpsc::Sender<(Level, LogData)>,
    priority_sender: &mpsc::Sender<(Level, LogData)>,
    dropped_entries: &AtomicU64,
    dropped_priority_entries: &AtomicU64,
    entry: (Level, LogData),
) {
    let (sender, dropped_entries) = if is_priority_level(&entry.0) {
        (priority_sender, dropped_priority_entries)
    } else {
        (sender, dropped_entries)
    };
    if sender.try_send(entry).is_ok() {
        return;
    }
    dropped_entries.fetch_add(1, Ordering::Relaxed);
}

const fn is_priority_level(level: &Level) -> bool {
    matches!(
        level,
        Level::Tracing(tracing::Level::WARN | tracing::Level::ERROR)
    )
}

fn take_shutdown_batch<T>(
    priority_receiver: &mut mpsc::Receiver<T>,
    receiver: &mut mpsc::Receiver<T>,
    normal_limit: usize,
) -> (Vec<T>, u64, u64) {
    priority_receiver.close();
    receiver.close();
    let mut pending = Vec::with_capacity(
        priority_receiver
            .len()
            .saturating_add(normal_limit.min(receiver.len())),
    );
    while let Ok(entry) = priority_receiver.try_recv() {
        pending.push(entry);
    }
    let priority_len = pending.len();
    while pending.len() - priority_len < normal_limit {
        let Ok(entry) = receiver.try_recv() else {
            break;
        };
        pending.push(entry);
    }
    (
        pending,
        priority_receiver.len() as u64,
        receiver.len() as u64,
    )
}

/// A logger layer for tracing
pub struct LoggerLayer(pub Arc<CommandLogger>);

impl LoggerLayer {
    /// Creates a new logger
    pub async fn new(
        cancel_token: CancellationToken,
        log_config: Option<LogConfig>,
    ) -> Result<Self, String> {
        Ok(Self(CommandLogger::init(cancel_token, log_config).await?))
    }
}

impl<S: Subscriber> Layer<S> for LoggerLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let mut data = LogData::new();
        event.record(&mut data);
        self.0.log(Level::Tracing(*event.metadata().level()), data);
    }
}

/// Whether a log line's structured fields should be rendered.
///
/// A warning or an error always shows them: the fields are the reason the line
/// was worth writing. `extra` is `#[serde(default)]`, so it is false in every
/// generated config -- which turned `Chunk scheduling epoch slow` and its
/// fifteen timings into a sentence that told nobody anything, on a real crash.
/// Anything quieter than a warning keeps the flag, so routine lines stay short.
fn should_render_extra(lvl: &Level, log_config: Option<&LogConfig>) -> bool {
    matches!(
        lvl,
        Level::Tracing(tracing::Level::WARN | tracing::Level::ERROR)
    ) || log_config.is_some_and(|l| l.extra)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use crate::config::{LogLevel, RotationTimeFormat};
    use tokio::sync::mpsc;

    use super::{
        LOG_PRIORITY_QUEUE_CAPACITY, LOG_QUEUE_CAPACITY, LOG_SHUTDOWN_DRAIN_LIMIT, Level,
        LogConfig, LogTimeFormat, enqueue_log, normalize_terminal_newlines, rendered_terminal_rows,
        should_render_extra, take_shutdown_batch,
    };
    use foton_utils::logger::LogData;

    #[test]
    fn logger_queue_drops_excess_instead_of_growing_without_bound() {
        let (sender, _receiver) = mpsc::channel(LOG_QUEUE_CAPACITY);
        let (priority_sender, _priority_receiver) = mpsc::channel(LOG_PRIORITY_QUEUE_CAPACITY);
        let dropped = AtomicU64::new(0);
        let dropped_priority = AtomicU64::new(0);
        for _ in 0..LOG_QUEUE_CAPACITY + 3 {
            enqueue_log(
                &sender,
                &priority_sender,
                &dropped,
                &dropped_priority,
                (Level::Tracing(tracing::Level::INFO), LogData::new()),
            );
        }
        assert_eq!(dropped.load(Ordering::Relaxed), 3);
        assert_eq!(dropped_priority.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn error_is_preserved_behind_a_saturated_info_queue() {
        let (sender, _receiver) = mpsc::channel(LOG_QUEUE_CAPACITY);
        let (priority_sender, mut priority_receiver) = mpsc::channel(LOG_PRIORITY_QUEUE_CAPACITY);
        let dropped = AtomicU64::new(0);
        let dropped_priority = AtomicU64::new(0);
        for _ in 0..LOG_QUEUE_CAPACITY {
            enqueue_log(
                &sender,
                &priority_sender,
                &dropped,
                &dropped_priority,
                (Level::Tracing(tracing::Level::INFO), LogData::new()),
            );
        }
        enqueue_log(
            &sender,
            &priority_sender,
            &dropped,
            &dropped_priority,
            (Level::Tracing(tracing::Level::ERROR), LogData::new()),
        );

        let (level, _) = priority_receiver
            .try_recv()
            .expect("ERROR should use the reserved priority capacity");
        assert!(matches!(level, Level::Tracing(tracing::Level::ERROR)));
        assert_eq!(dropped_priority.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn shutdown_closes_producers_and_caps_the_drain() {
        let (sender, mut receiver) = mpsc::channel(LOG_SHUTDOWN_DRAIN_LIMIT + 3);
        let (priority_sender, mut priority_receiver) = mpsc::channel(1);
        for sequence in 0..LOG_SHUTDOWN_DRAIN_LIMIT + 3 {
            sender
                .try_send(sequence)
                .expect("the shutdown fixture should fit in the queue");
        }
        priority_sender
            .try_send(usize::MAX)
            .expect("priority fixture should fit");

        let (pending, abandoned_priority, abandoned) = take_shutdown_batch(
            &mut priority_receiver,
            &mut receiver,
            LOG_SHUTDOWN_DRAIN_LIMIT,
        );

        assert_eq!(pending.len(), LOG_SHUTDOWN_DRAIN_LIMIT + 1);
        assert_eq!(pending[0], usize::MAX);
        assert_eq!(abandoned_priority, 0);
        assert_eq!(abandoned, 3);
        assert!(sender.try_send(0).is_err(), "shutdown must close producers");
        assert!(
            priority_sender.try_send(0).is_err(),
            "shutdown must close priority producers"
        );
    }

    fn config_with_extra(extra: bool) -> LogConfig {
        LogConfig {
            log_path: String::new(),
            log_level: LogLevel::default(),
            time: LogTimeFormat::default(),
            module_path: false,
            extra,
            log_file: false,
            rotation_time: RotationTimeFormat::default(),
            max_history: 0,
        }
    }

    /// A warning carries its fields because the fields are the point.
    ///
    /// `extra` defaults to false and is written `extra = false` into every
    /// generated config, so before this a fifteen-field slow-epoch warning
    /// printed nothing but its sentence.
    #[test]
    fn a_warning_shows_its_fields_even_with_extra_off() {
        let off = config_with_extra(false);
        assert!(should_render_extra(
            &Level::Tracing(tracing::Level::WARN),
            Some(&off)
        ));
        assert!(should_render_extra(
            &Level::Tracing(tracing::Level::ERROR),
            Some(&off)
        ));
        assert!(should_render_extra(
            &Level::Tracing(tracing::Level::WARN),
            None
        ));
    }

    /// Anything quieter keeps the flag, so routine lines stay readable.
    #[test]
    fn a_routine_line_still_obeys_the_flag() {
        assert!(!should_render_extra(
            &Level::Tracing(tracing::Level::INFO),
            Some(&config_with_extra(false))
        ));
        assert!(should_render_extra(
            &Level::Tracing(tracing::Level::INFO),
            Some(&config_with_extra(true))
        ));
        assert!(!should_render_extra(
            &Level::Tracing(tracing::Level::DEBUG),
            None
        ));
    }

    #[test]
    fn rendered_rows_include_wrapping_and_trailing_newlines() {
        assert_eq!(rendered_terminal_rows("short", 10), 1);
        assert_eq!(rendered_terminal_rows("12345678901", 10), 2);
        assert_eq!(rendered_terminal_rows("line\n", 10), 2);
        assert_eq!(rendered_terminal_rows("first\nsecond", 10), 2);
    }

    #[test]
    fn rendered_rows_ignore_ansi_and_count_tabs_conservatively() {
        assert_eq!(rendered_terminal_rows("\u{1b}[31mshort\u{1b}[0m", 10), 1);
        assert_eq!(rendered_terminal_rows("\t", 4), 2);
    }

    #[test]
    fn terminal_newlines_reset_the_output_column() {
        assert_eq!(
            normalize_terminal_newlines("first\nsecond"),
            "first\r\nsecond"
        );
        assert_eq!(
            normalize_terminal_newlines("first\r\nsecond"),
            "first\r\nsecond"
        );
        assert_eq!(rendered_terminal_rows("12345\r\n123456", 10), 2);
    }
}
