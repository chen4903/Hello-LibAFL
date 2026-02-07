use libafl::corpus::{Corpus, HasCurrentCorpusId, InMemoryOnDiskCorpus};
use libafl::executors::InProcessExecutor;
use libafl::inputs::BytesInput;
use libafl::monitors::SimpleMonitor;
use libafl::stages::{ObserverEqualityFactory, StagesTuple, StdTMinMutationalStage};
use libafl::state::HasSolutions;
use libafl::Error;
use libafl::{
    corpus::{InMemoryCorpus, OnDiskCorpus},
    events::SimpleEventManager,
    executors::ExitKind,
    feedbacks::{CrashFeedback, MaxMapFeedback},
    fuzzer::{Fuzzer, StdFuzzer},
    generators::RandPrintablesGenerator,
    inputs::HasTargetBytes,
    mutators::{havoc_mutations::havoc_mutations, scheduled::HavocScheduledMutator},
    observers::ConstMapObserver,
    schedulers::QueueScheduler,
    stages::mutational::StdMutationalStage,
    state::{HasCorpus, StdState},
};
use libafl_bolts::{nonnull_raw_mut, nonzero, rands::StdRand, tuples::tuple_list, AsSlice};
use std::fs;
use std::path::Path;
use std::{path::PathBuf, ptr::write};

/// Coverage map with explicit assignments due to the lack of instrumentation
const SIGNALS_LEN: usize = 16;
static mut SIGNALS: [u8; SIGNALS_LEN] = [0; SIGNALS_LEN];
static mut SIGNALS_PTR: *mut u8 = &raw mut SIGNALS as _;

/// Assign a signal to the signals map
fn signals_set(idx: usize) {
    unsafe { write(SIGNALS_PTR.add(idx), 1) };
}

/// Print inputs from a directory
///
/// Reads all files in the given directory and displays them sorted by size.
/// Skips files starting with '.' and files ending with '.metadata'.
fn print_inputs_from_dir(dir: &PathBuf, title: &str) {
    println!("\n=== {} ===", title);
    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut inputs = Vec::new();
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                // Skip hidden files and metadata files
                if let Some(file_name) = path.file_name() {
                    if let Some(name_str) = file_name.to_str() {
                        if !name_str.starts_with('.') && !name_str.ends_with(".metadata") {
                            if let Ok(content) = std::fs::read(&path) {
                                inputs.push((name_str.to_string(), content));
                            }
                        }
                    }
                }
            }
        }

        // Sort by size for better readability
        inputs.sort_by_key(|(_, content)| content.len());

        println!("Found {} inputs:", inputs.len());
        for (i, (_name, content)) in inputs.iter().enumerate() {
            // Try to display as UTF-8 string, fallback to raw bytes
            match std::str::from_utf8(content) {
                Ok(s) => println!(
                    "  [{}] Size: {} bytes, Content: \"{}\"",
                    i + 1,
                    content.len(),
                    s
                ),
                Err(_) => println!(
                    "  [{}] Size: {} bytes, Content: {:?}",
                    i + 1,
                    content.len(),
                    content
                ),
            }
        }
    }
}

/// The closure that we want to fuzz
fn harness(input: &BytesInput) -> ExitKind {
    let target = input.target_bytes();
    let buf = target.as_slice();
    signals_set(0);
    if !buf.is_empty() && buf[0] == b'a' {
        signals_set(1);
        if buf.len() > 1 && buf[1] == b'b' {
            signals_set(2);
            if buf.len() > 2 && buf[2] == b'c' {
                return ExitKind::Crash;
            }
        }
    }
    ExitKind::Ok
}

pub fn main() -> Result<(), Error> {
    delete_cache_files().expect("Failed to delete cache files");

    // The closure that we want to fuzz
    let mut to_fuzz = harness;

    // Create an observation channel using the signals map
    let observer = unsafe { ConstMapObserver::from_mut_ptr("signals", nonnull_raw_mut!(SIGNALS)) };

    let factory = ObserverEqualityFactory::new(&observer);

    // Feedback to rate the interestingness of an input
    let mut feedback = MaxMapFeedback::new(&observer);

    // A feedback to choose if an input is a solution or not
    let mut objective = CrashFeedback::new();

    // The Monitor trait define how the fuzzer stats are displayed to the user
    let mon = SimpleMonitor::new(|s| println!("{s}"));

    let mut mgr = SimpleEventManager::new(mon);

    let corpus_dir = PathBuf::from("./corpus");
    let solution_dir = PathBuf::from("./solutions");

    // create a State from scratch
    let mut state = StdState::new(
        // RNG
        StdRand::new(),
        // Corpus that will be evolved, we keep it in memory for performance
        InMemoryOnDiskCorpus::new(corpus_dir).unwrap(),
        // Corpus in which we store solutions (crashes in this example),
        // on disk so the user can get them after stopping the fuzzer
        OnDiskCorpus::new(&solution_dir).unwrap(),
        // States of the feedbacks.
        // The feedbacks can report the data that should persist in the State.
        &mut feedback,
        // Same for objective feedbacks
        &mut objective,
    )
    .unwrap();

    // A queue policy to get testcasess from the corpus
    let scheduler = QueueScheduler::new();

    // A fuzzer with feedbacks and a corpus scheduler
    let mut fuzzer = StdFuzzer::new(scheduler, feedback, objective);

    // Create the executor for an in-process function with just one observer
    let mut executor = InProcessExecutor::new(
        &mut to_fuzz,
        tuple_list!(observer),
        &mut fuzzer,
        &mut state,
        &mut mgr,
    )
    .expect("Failed to create the Executor");

    // Generator of printable bytearrays of max size 32
    let mut generator = RandPrintablesGenerator::new(nonzero!(32));

    // Generate 8 initial inputs
    state
        .generate_initial_inputs(&mut fuzzer, &mut executor, &mut generator, &mut mgr, 8)
        .expect("Failed to generate the initial corpus");

    // Setup a mutational stage with a basic bytes mutator
    let mutator = HavocScheduledMutator::new(havoc_mutations());
    let minimizer = HavocScheduledMutator::new(havoc_mutations());
    let mut stages = tuple_list!(
        StdMutationalStage::new(mutator),
        StdTMinMutationalStage::new(minimizer, factory, 128)
    );

    while state.solutions().is_empty() {
        fuzzer.fuzz_one(&mut stages, &mut executor, &mut state, &mut mgr)?;
    }

    // ============================== Start minimization ==============================

    let minimized_dir = PathBuf::from("./minimized");

    let mut state = StdState::new(
        StdRand::new(),
        InMemoryOnDiskCorpus::new(&minimized_dir).unwrap(),
        InMemoryCorpus::new(),
        &mut (),
        &mut (),
    )
    .unwrap();

    // The Monitor trait define how the fuzzer stats are displayed to the user
    let mon = SimpleMonitor::new(|s| println!("{s}"));

    let mut mgr = SimpleEventManager::new(mon);

    // Print crash inputs before minimization
    print_inputs_from_dir(&solution_dir, "Crash Inputs Before Minimization");

    let minimizer = HavocScheduledMutator::new(havoc_mutations());
    let mut stages = tuple_list!(StdTMinMutationalStage::new(
        minimizer,
        CrashFeedback::new(),
        1 << 10,
    ));

    let scheduler = QueueScheduler::new();

    // A fuzzer with feedbacks and a corpus scheduler
    let mut fuzzer = StdFuzzer::new(scheduler, (), ());

    // Create the executor for an in-process function with just one observer
    let mut executor = InProcessExecutor::new(&mut to_fuzz, (), &mut fuzzer, &mut state, &mut mgr)?;

    state.load_initial_inputs_forced(&mut fuzzer, &mut executor, &mut mgr, &[solution_dir])?;

    let first_id = state.corpus().first().expect("Empty corpus");
    state.set_corpus_id(first_id)?;

    println!("\n=== Starting Minimization ===");
    stages.perform_all(&mut fuzzer, &mut executor, &mut state, &mut mgr)?;

    // Print minimized crash inputs
    print_inputs_from_dir(&minimized_dir, "Minimized Crash Inputs");

    Ok(())
}

pub fn delete_cache_files() -> std::io::Result<()> {
    let dirs = vec!["crashes", "corpus", "solutions", "minimized"];

    for dir_name in dirs {
        if Path::new(dir_name).exists() {
            fs::remove_dir_all(dir_name)?;
            println!("Deleted directory: {}/", dir_name);
        }
    }

    Ok(())
}
