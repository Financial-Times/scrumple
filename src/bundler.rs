use crate::input_options::InputOptions;
use crate::modules::{Module, ModuleState};
use crate::resolver::Resolved;
use crate::source_maps::SourceMapOutput;
use crate::worker::{Work, WorkDone, Worker, WorkerInit};
use crate::writer::Writer;
use crate::CliError;
use crossbeam::sync::SegQueue;
use fnv::FnvHashMap;
use matches::debug_assert_matches;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::{fs, io};

pub fn bundle(
    entry_point: &Path,
    input_options: InputOptions,
    output: &str,
    map_output: &SourceMapOutput,
) -> Result<FnvHashMap<PathBuf, Module>, CliError> {
    let mut pending = 0;
    let thread_count = num_cpus::get();
    let (tx, rx) = mpsc::channel();
    let worker_init = WorkerInit {
        tx,
        input_options,
        quit: Arc::new(AtomicBool::new(false)),
        queue: Arc::new(SegQueue::new()),
    };

    // TODO: context.require('…')
    // TODO: watch for missing files on error?

    let mut modules = FnvHashMap::<PathBuf, ModuleState>::default();

    worker_init.add_work(Work::Include {
        module: entry_point.to_owned(),
    });
    pending += 1;
    modules.insert(entry_point.to_owned(), ModuleState::Loading);

    let children: Vec<_> = (0..thread_count)
        .map(|_| {
            let init = worker_init.clone();
            thread::spawn(move || Worker::new(init).run())
        })
        .collect();
    // let children: Vec<_> = (0..thread_count).map(|n| {    //     let init = worker_init.clone();
    //     thread::Builder::new().name(format!("worker #{}", n + 1)).spawn(move || Worker::new(init).run()).unwrap()
    // }).collect();

    while let Ok(work_done) = rx.recv() {
        // eprintln!("{:?}", work_done);
        let work_done = match work_done {
            Err(error) => {
                worker_init.quit.store(true, Ordering::Relaxed);
                return Err(error);
            }
            Ok(work_done) => {
                pending -= 1;
                work_done
            }
        };
        match work_done {
            WorkDone::Resolve {
                context,
                name,
                resolved,
            } => {
                match *modules.get_mut(&context).unwrap() {
                    ModuleState::Loading => unreachable!(),
                    ModuleState::Loaded(Module { ref mut deps, .. }) => {
                        deps.insert(name, resolved.clone());
                    }
                }
                match resolved {
                    Resolved::External => {}
                    Resolved::Ignore => {}
                    Resolved::Normal(module) => {
                        modules.entry(module.clone()).or_insert_with(|| {
                            worker_init.add_work(Work::Include { module });
                            pending += 1;
                            ModuleState::Loading
                        });
                    }
                }
            }
            WorkDone::Include { module, info } => {
                let old = modules.insert(
                    module.clone(),
                    ModuleState::Loaded(Module {
                        source: info.source,
                        deps: FnvHashMap::default(),
                    }),
                );
                debug_assert_matches!(old, Some(ModuleState::Loading));
                for dep in info.deps {
                    worker_init.add_work(Work::Resolve {
                        context: module.clone(),
                        name: dep,
                    });
                    pending += 1;
                }
            }
        }
        if pending == 0 {
            break;
        }
    }

    worker_init.quit.store(true, Ordering::Relaxed);
    for child in children {
        child.join()?;
    }

    let writer = Writer {
        modules: modules
            .into_iter()
            .map(|(k, ms)| {
                let parent = entry_point.parent().unwrap();
                let ms = ms.unwrap();

                match k.as_path().strip_prefix(parent) {
                    Ok(path) => (PathBuf::from(path), ms),
                    Err(_) => (k, ms),
                }
            })
            .collect(),
        entry_point,
        map_output,
    };

    match &*output {
        "-" => {
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            writer.write_to(&mut handle)?;
        }
        _ => {
            let output = Path::new(output);
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent)?;
            }
            let file = fs::File::create(&output)?;
            let mut buf_writer = io::BufWriter::new(file);
            writer.write_to(&mut buf_writer)?;
        }
    }
    match *map_output {
        SourceMapOutput::Suppressed => {}
        SourceMapOutput::Inline => {
            // handled in Writer::write_to()
        }
        SourceMapOutput::File(ref path, _) => {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let file = fs::File::create(path)?;
            let mut buf_writer = io::BufWriter::new(file);
            writer.write_map_to(&mut buf_writer)?;
        }
    }
    // println!("entry point: {:?}", entry_point);
    // println!("{:#?}", modules);

    Ok(writer.modules)
}
