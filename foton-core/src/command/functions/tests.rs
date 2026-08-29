use std::{
    env::temp_dir,
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicI32, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use foton_utils::{Identifier, locks::SyncMutex};
use rustc_hash::FxHashMap;
use simdnbt::owned::{NbtCompound, NbtTag};

use super::super::brigadier::{
    ArgumentType, CommandDispatcher, CommandNodeBuilder, CommandSyntaxError,
};
use super::super::execution::{
    CommandArgumentSource, CommandExecutionContext, CommandResultCallback, ExecutionCommandSource,
    ExecutionStop, FotonCommandRuntime, FunctionEntries, argument, literal,
};
use super::library::{CommandFunction, FunctionLibrary};
use super::loader::{self, TagEntry};
use super::macros::MacroFunction;
use super::manager::build_tags;
use super::parser::{FunctionBody, FunctionParseError, parse_body};

/// A source whose only job is to record what the compiled lines did.
#[derive(Clone)]
struct TestSource {
    log: Arc<SyncMutex<Vec<i32>>>,
    callback: CommandResultCallback,
}

impl TestSource {
    fn new() -> Self {
        Self {
            log: Arc::new(SyncMutex::new(Vec::new())),
            callback: CommandResultCallback::empty(),
        }
    }
}

impl CommandArgumentSource for TestSource {}

impl ExecutionCommandSource for TestSource {
    fn with_callback(&self, callback: CommandResultCallback) -> Self {
        Self {
            log: Arc::clone(&self.log),
            callback,
        }
    }

    fn callback(&self) -> CommandResultCallback {
        self.callback.clone()
    }

    fn handle_error(&self, _error: &CommandSyntaxError, _forked: bool) {}
}

type TestDispatcher = CommandDispatcher<TestSource, FotonCommandRuntime>;

/// Registers `record <value>`, which appends its argument to the source's log.
fn test_dispatcher() -> TestDispatcher {
    let mut dispatcher = TestDispatcher::new();
    let builder: CommandNodeBuilder<TestSource, FotonCommandRuntime> =
        literal::<TestSource>("record").then(
            argument::<TestSource>("value", ArgumentType::integer(-100, 100)).executes(|context| {
                let Ok(value) = context.integer("value") else {
                    panic!("the record argument should be parsed");
                };
                context.source().log.lock().push(value);
                Ok(value)
            }),
        );
    let Ok(_) = dispatcher.register(builder) else {
        panic!("test command should register");
    };
    dispatcher
}

fn unique_root(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    temp_dir().join(format!("foton-functions-{name}-{unique}"))
}

fn write(path: &PathBuf, contents: &str) {
    let Some(parent) = path.parent() else {
        panic!("resource paths always have a parent directory");
    };
    let Ok(()) = fs::create_dir_all(parent) else {
        panic!("test datapack directory should be creatable");
    };
    let Ok(()) = fs::write(path, contents) else {
        panic!("test datapack file should be writable");
    };
}

fn function(id: &str) -> Arc<CommandFunction> {
    let Ok(id) = id.parse::<Identifier>() else {
        panic!("test function ids are valid");
    };
    Arc::new(CommandFunction::new(
        id,
        FunctionBody::Plain(Vec::new().into()),
    ))
}

/// Compiles a body that is expected to have no macro lines.
fn parse_entries(
    body: &str,
    dispatcher: &TestDispatcher,
    source: &TestSource,
) -> Result<FunctionEntries<TestSource>, FunctionParseError> {
    match parse_body(body, dispatcher, source)? {
        FunctionBody::Plain(entries) => Ok(entries),
        FunctionBody::Macro(_) => panic!("this body was not supposed to contain macro lines"),
    }
}

/// Compiles a body that is expected to have at least one macro line.
fn parse_macro(
    body: &str,
    dispatcher: &TestDispatcher,
    source: &TestSource,
) -> MacroFunction<TestSource> {
    match parse_body(body, dispatcher, source) {
        Ok(FunctionBody::Macro(function)) => function,
        Ok(FunctionBody::Plain(_)) => panic!("this body was supposed to contain macro lines"),
        Err(error) => panic!("macro body should compile: {error}"),
    }
}

fn tag_entry(id: &str, references_tag: bool, required: bool) -> TagEntry {
    let Ok(id) = id.parse::<Identifier>() else {
        panic!("test tag entry ids are valid");
    };
    TagEntry {
        id,
        references_tag,
        required,
        source_pack: "test".to_owned(),
    }
}

fn identifier(id: &str) -> Identifier {
    let Ok(id) = id.parse::<Identifier>() else {
        panic!("test identifiers are valid");
    };
    id
}

/// Runs a compiled function body and returns what its lines recorded.
fn run(body: &str) -> Vec<i32> {
    let dispatcher = test_dispatcher();
    let source = TestSource::new();
    let entries = match parse_entries(body, &dispatcher, &source) {
        Ok(entries) => entries,
        Err(error) => panic!("function body should compile: {error}"),
    };
    let log = Arc::clone(&source.log);
    let mut execution = CommandExecutionContext::new(100, 100);
    execution.queue_initial_function_call(entries, source, CommandResultCallback::empty());
    assert_eq!(execution.run(), ExecutionStop::Completed);
    snapshot(&log)
}

/// Reads back a `SyncMutex`-guarded log without holding the guard.
fn snapshot(log: &Arc<SyncMutex<Vec<i32>>>) -> Vec<i32> {
    log.lock().clone()
}

#[test]
fn compiled_lines_run_in_file_order_and_skip_comments_and_blanks() {
    let recorded = run("# a comment\n\nrecord 1\n   \nrecord 2\n#record 99\nrecord 3\n");
    assert_eq!(recorded, vec![1, 2, 3]);
}

#[test]
fn more_than_two_lines_still_run_in_order_through_the_continuation() {
    // Three or more entries take the continuation path rather than being
    // queued one by one, so their order is worth pinning down.
    let recorded = run("record 1\nrecord 2\nrecord 3\nrecord 4\nrecord 5\n");
    assert_eq!(recorded, vec![1, 2, 3, 4, 5]);
}

#[test]
fn a_trailing_backslash_joins_the_next_line_into_one_command() {
    let recorded = run("record \\\n7\nrecord 8\n");
    assert_eq!(recorded, vec![7, 8]);
}

#[test]
fn a_leading_slash_is_rejected_at_the_line_it_appears_on() {
    let dispatcher = test_dispatcher();
    let source = TestSource::new();
    let Err(error) = parse_entries("record 1\n/record 2\n", &dispatcher, &source) else {
        panic!("a slash-prefixed command should not compile");
    };
    assert_eq!(error.line(), 2);
    assert!(
        error.to_string().contains("record"),
        "the error should quote the command that was meant: {error}"
    );
}

#[test]
fn a_macro_body_substitutes_its_arguments_and_keeps_its_plain_lines() {
    let dispatcher = test_dispatcher();
    let source = TestSource::new();
    let function = parse_macro("record 1\n$record $(value)\n", &dispatcher, &source);
    let mut arguments = NbtCompound::new();
    arguments.insert("value", NbtTag::Int(42));
    let Ok(entries) =
        function.instantiate(&identifier("test:macro"), Some(&arguments), &dispatcher)
    else {
        panic!("the macro should instantiate with its argument supplied");
    };

    let log = Arc::clone(&source.log);
    let mut execution = CommandExecutionContext::new(100, 100);
    execution.queue_initial_function_call(entries, source, CommandResultCallback::empty());
    assert_eq!(execution.run(), ExecutionStop::Completed);
    assert_eq!(snapshot(&log), vec![1, 42]);
}

#[test]
fn a_macro_reuses_one_instantiation_for_the_same_arguments() {
    let dispatcher = test_dispatcher();
    let source = TestSource::new();
    let function = parse_macro("$record $(value)\n", &dispatcher, &source);
    let mut arguments = NbtCompound::new();
    arguments.insert("value", NbtTag::Int(3));
    let id = identifier("test:macro");
    let (Ok(first), Ok(second)) = (
        function.instantiate(&id, Some(&arguments), &dispatcher),
        function.instantiate(&id, Some(&arguments), &dispatcher),
    ) else {
        panic!("both instantiations should succeed");
    };
    assert!(
        Arc::ptr_eq(&first, &second),
        "the second call should come back from the cache"
    );
}

#[test]
fn a_macro_called_without_its_argument_fails_instead_of_substituting_nothing() {
    let dispatcher = test_dispatcher();
    let source = TestSource::new();
    let function = parse_macro("$record $(value)\n", &dispatcher, &source);
    let id = identifier("test:macro");

    assert!(
        function.instantiate(&id, None, &dispatcher).is_err(),
        "a macro with no arguments at all must not run"
    );
    assert!(
        function
            .instantiate(&id, Some(&NbtCompound::new()), &dispatcher)
            .is_err(),
        "a macro missing one of its arguments must not run"
    );
}

#[test]
fn a_macro_line_that_substitutes_into_nonsense_fails_at_the_call() {
    let dispatcher = test_dispatcher();
    let source = TestSource::new();
    let function = parse_macro("$$(command) 1\n", &dispatcher, &source);
    let mut arguments = NbtCompound::new();
    arguments.insert("command", NbtTag::String("nosuchcommand".into()));
    assert!(
        function
            .instantiate(&identifier("test:macro"), Some(&arguments), &dispatcher)
            .is_err(),
        "a substitution that is not a command must not be queued"
    );
}

#[test]
fn a_macro_line_with_no_variable_is_a_load_error() {
    let dispatcher = test_dispatcher();
    let source = TestSource::new();
    let Err(error) = parse_body("record 1\n$record 2\n", &dispatcher, &source) else {
        panic!("a macro line with nothing to substitute should not compile");
    };
    assert_eq!(error.line(), 2);
}

#[test]
fn an_unknown_command_fails_the_whole_function_at_its_line() {
    let dispatcher = test_dispatcher();
    let source = TestSource::new();
    let Err(error) = parse_entries("record 1\nrecord 2\nnosuchcommand\n", &dispatcher, &source)
    else {
        panic!("an unknown command should not compile");
    };
    assert_eq!(error.line(), 3);
}

#[test]
fn a_function_reports_its_last_result_to_the_caller() {
    let dispatcher = test_dispatcher();
    let source = TestSource::new();
    let Ok(entries) = parse_entries("record 4\nrecord 9\n", &dispatcher, &source) else {
        panic!("function body should compile");
    };
    let results = Arc::new(SyncMutex::new(Vec::new()));
    let collected = Arc::clone(&results);
    let callback = CommandResultCallback::new(move |success, result| {
        collected.lock().push((success, result));
    });
    let mut execution = CommandExecutionContext::new(100, 100);
    execution.queue_initial_function_call(
        entries,
        source.with_callback(callback),
        CommandResultCallback::empty(),
    );
    assert_eq!(execution.run(), ExecutionStop::Completed);
    let observed = results.lock().clone();
    assert_eq!(observed, vec![(true, 4), (true, 9)]);
}

#[test]
fn a_function_call_costs_one_command_plus_its_lines() {
    // Vanilla charges the call itself before its body, which is what stops a
    // function that only calls functions from recursing without limit.
    let dispatcher = test_dispatcher();
    let source = TestSource::new();
    let Ok(entries) = parse_entries("record 1\nrecord 2\nrecord 3\n", &dispatcher, &source) else {
        panic!("function body should compile");
    };
    let log = Arc::clone(&source.log);
    let mut execution = CommandExecutionContext::new(3, 100);
    execution.queue_initial_function_call(entries, source, CommandResultCallback::empty());
    assert_eq!(execution.run(), ExecutionStop::CommandLimit);
    assert_eq!(
        snapshot(&log),
        vec![1, 2],
        "the call plus two lines should exhaust a budget of three"
    );
}

#[test]
fn tags_resolve_nested_tags_and_drop_duplicates() {
    let mut functions = FxHashMap::default();
    for id in ["test:a", "test:b"] {
        functions.insert(identifier(id), function(id));
    }
    let mut raw = FxHashMap::default();
    raw.insert(
        identifier("test:inner"),
        vec![tag_entry("test:a", false, true)],
    );
    raw.insert(
        identifier("test:outer"),
        vec![
            tag_entry("test:inner", true, true),
            tag_entry("test:a", false, true),
            tag_entry("test:b", false, true),
        ],
    );
    let mut errors = Vec::new();
    let tags = build_tags(&raw, &functions, &mut errors);
    assert!(errors.is_empty(), "no entry is missing: {errors:?}");

    let library = FunctionLibrary::new(functions, tags);
    let names = library
        .tag(&identifier("test:outer"))
        .iter()
        .map(|function| function.id().to_string())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["test:a".to_owned(), "test:b".to_owned()]);
}

#[test]
fn a_tag_missing_a_required_entry_is_dropped_rather_than_left_empty() {
    let mut functions = FxHashMap::default();
    functions.insert(identifier("test:a"), function("test:a"));
    let mut raw = FxHashMap::default();
    raw.insert(
        identifier("test:broken"),
        vec![
            tag_entry("test:a", false, true),
            tag_entry("test:missing", false, true),
        ],
    );
    let mut errors = Vec::new();
    let tags = build_tags(&raw, &functions, &mut errors);

    assert_eq!(errors.len(), 1, "the missing entry should be reported");
    assert!(
        !tags.contains_key(&identifier("test:broken")),
        "a tag with a missing required entry must not exist at all, \
         because an existing but empty tag would silently run nothing"
    );
}

#[test]
fn an_optional_missing_entry_only_removes_itself() {
    let mut functions = FxHashMap::default();
    functions.insert(identifier("test:a"), function("test:a"));
    let mut raw = FxHashMap::default();
    raw.insert(
        identifier("test:partial"),
        vec![
            tag_entry("test:missing", false, false),
            tag_entry("test:a", false, true),
        ],
    );
    let mut errors = Vec::new();
    let tags = build_tags(&raw, &functions, &mut errors);

    assert!(errors.is_empty(), "an optional entry is not an error");
    let Some(tag) = tags.get(&identifier("test:partial")) else {
        panic!("the tag should still exist");
    };
    assert_eq!(tag.len(), 1);
}

#[test]
fn a_tag_reference_cycle_is_reported_instead_of_recursing() {
    let mut raw = FxHashMap::default();
    raw.insert(
        identifier("test:one"),
        vec![tag_entry("test:two", true, true)],
    );
    raw.insert(
        identifier("test:two"),
        vec![tag_entry("test:one", true, true)],
    );
    let mut errors = Vec::new();
    let tags = build_tags(&raw, &FxHashMap::default(), &mut errors);

    assert!(!errors.is_empty(), "the cycle should be reported");
    assert!(tags.is_empty(), "neither side of a cycle can be built");
}

#[test]
fn a_datapack_scan_finds_nested_functions_and_tags() {
    let root = unique_root("scan");
    let pack = root.join("example");
    write(
        &pack.join("data/test/function/greet.mcfunction"),
        "record 1\n",
    );
    write(
        &pack.join("data/test/function/nested/deep.mcfunction"),
        "record 2\n",
    );
    write(
        &pack.join("data/minecraft/tags/function/tick.json"),
        "{\"values\": [\"test:greet\", {\"id\": \"test:absent\", \"required\": false}]}",
    );

    let contents = loader::collect(&root);
    let _ = fs::remove_dir_all(&root);

    assert!(contents.errors.is_empty(), "{:?}", contents.errors);
    let mut found = contents
        .functions
        .keys()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    found.sort();
    assert_eq!(
        found,
        vec!["test:greet".to_owned(), "test:nested/deep".to_owned()]
    );

    let Some(tick) = contents.tags.get(&identifier("minecraft:tick")) else {
        panic!("the tick tag should be read");
    };
    assert_eq!(tick.len(), 2);
    assert!(tick[0].required, "a bare entry is required");
    assert!(!tick[1].required, "an explicit required:false is optional");
}

#[test]
fn a_missing_datapack_directory_is_not_an_error() {
    let contents = loader::collect(&unique_root("absent"));
    assert!(contents.errors.is_empty());
    assert!(contents.functions.is_empty());
}

#[test]
fn a_later_pack_replaces_an_earlier_packs_function() {
    let root = unique_root("override");
    write(
        &root.join("a_pack/data/test/function/greet.mcfunction"),
        "record 1\n",
    );
    write(
        &root.join("b_pack/data/test/function/greet.mcfunction"),
        "record 2\n",
    );

    let contents = loader::collect(&root);
    let _ = fs::remove_dir_all(&root);

    let Some(source) = contents.functions.get(&identifier("test:greet")) else {
        panic!("the function should be loaded once");
    };
    assert_eq!(source.source_pack, "b_pack");
    assert_eq!(source.text.trim(), "record 2");
}

#[test]
fn a_replacing_tag_file_discards_the_entries_of_earlier_packs() {
    let root = unique_root("replace");
    write(
        &root.join("a_pack/data/minecraft/tags/function/tick.json"),
        "{\"values\": [\"test:first\"]}",
    );
    write(
        &root.join("b_pack/data/minecraft/tags/function/tick.json"),
        "{\"replace\": true, \"values\": [\"test:second\"]}",
    );

    let contents = loader::collect(&root);
    let _ = fs::remove_dir_all(&root);

    let Some(tick) = contents.tags.get(&identifier("minecraft:tick")) else {
        panic!("the tick tag should be read");
    };
    let ids = tick
        .iter()
        .map(|entry| entry.id.to_string())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["test:second".to_owned()]);
}

#[test]
fn an_unreadable_tag_file_is_reported_and_skipped() {
    let root = unique_root("bad-tag");
    write(
        &root.join("pack/data/minecraft/tags/function/tick.json"),
        "{ not json",
    );

    let contents = loader::collect(&root);
    let _ = fs::remove_dir_all(&root);

    assert_eq!(contents.errors.len(), 1);
    assert!(contents.tags.is_empty());
}

/// Every line of a function reports its own result, so a caller that sums them
/// sees the total rather than only the last one.
#[test]
fn queued_function_results_are_summed_by_the_caller() {
    let dispatcher = test_dispatcher();
    let source = TestSource::new();
    let Ok(entries) = parse_entries("record 5\nrecord 6\n", &dispatcher, &source) else {
        panic!("function body should compile");
    };
    let total = Arc::new(AtomicI32::new(0));
    let counter = Arc::clone(&total);
    let callback = CommandResultCallback::new(move |_success, result| {
        counter.fetch_add(result, Ordering::Relaxed);
    });
    let mut execution = CommandExecutionContext::new(100, 100);
    execution.queue_initial_function_call(
        entries,
        source.with_callback(callback),
        CommandResultCallback::empty(),
    );
    assert_eq!(execution.run(), ExecutionStop::Completed);
    assert_eq!(total.load(Ordering::Relaxed), 11);
}
