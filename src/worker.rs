use crate::es6;
use crate::input_options::InputOptions;
use crate::modules::{self, ModuleInfo};
use crate::resolver::{Resolved, Resolver};
use crate::CliError;
use crossbeam::queue::SegQueue;
use esparse::lex;
use fnv::FnvHashSet;
use matches::matches;
use std::io::{self, Read};
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

#[derive(Debug)]
pub enum Work {
    Resolve { context: PathBuf, name: String },
    Include { module: PathBuf },
}

#[derive(Debug)]
pub enum WorkDone {
    Resolve {
        context: PathBuf,
        name: String,
        resolved: Resolved,
    },
    Include {
        module: PathBuf,
        info: ModuleInfo,
    },
}

#[derive(Debug, Clone)]
pub struct WorkerInit {
    pub tx: mpsc::Sender<Result<WorkDone, CliError>>,
    pub input_options: InputOptions,
    pub queue: Arc<SegQueue<Work>>,
    pub quit: Arc<AtomicBool>,
}

#[derive(Debug, Clone)]
pub struct Worker {
    tx: mpsc::Sender<Result<WorkDone, CliError>>,
    pub resolver: Resolver,
    queue: Arc<SegQueue<Work>>,
    quit: Arc<AtomicBool>,
}

impl WorkerInit {
    pub fn add_work(&self, work: Work) {
        self.queue.push(work);
    }
}

impl Worker {
    pub fn new(init: WorkerInit) -> Self {
        Worker {
            tx: init.tx,
            resolver: Resolver::new(init.input_options),
            queue: init.queue,
            quit: init.quit,
        }
    }

    pub fn run(mut self) {
        while let Some(work) = self.get_work() {
            let work_done = match work {
                Work::Resolve { context, name } => {
                    self.resolver
                        .resolve(&context, &name)
                        .map(|resolved| WorkDone::Resolve {
                            context,
                            name,
                            resolved,
                        })
                }
                Work::Include { module } => self
                    .include(&module)
                    .map(|info| WorkDone::Include { module, info }),
            };
            if self.tx.send(work_done).is_err() {
                return;
            }
        }
    }

    fn include(&self, module: &Path) -> Result<ModuleInfo, CliError> {
        let source = {
            let file = std::fs::File::open(module)?;
            let mut buf_reader = io::BufReader::new(file);
            let mut bytes = Vec::new();
            buf_reader.read_to_end(&mut bytes)?;
            match String::from_utf8(bytes) {
                Ok(s) => s,
                Err(err) => {
                    return Err(CliError::InvalidUtf8 {
                        context: module.to_owned(),
                        err,
                    });
                }
            }
        };
        let mut new_source = None;
        let prefix;
        let suffix;

        let deps = {
            let path_string = module.to_string_lossy();
            // module.to_str().ok_or("<path with invalid utf-8>")
            let mut lexer = lex::Lexer::new(path_string.as_ref(), &source);

            let deps;
            let ext = module.extension();
            if matches!(ext, Some(s) if s == "mjs") {
                let module = es6::module_to_cjs(&mut lexer, false)?;
                // println!("{:#?}", module);
                deps = module.deps;
                prefix = module.source_prefix;
                suffix = module.source_suffix;
                new_source = Some(module.source);
            } else if matches!(ext, Some(s) if s == "json") {
                deps = FnvHashSet::default();
                prefix = "module.exports =".to_owned();
                suffix = String::new();
            } else {
                let module = es6::module_to_cjs(&mut lexer, true)?;
                deps = module.deps;
                prefix = module.source_prefix;
                suffix = module.source_suffix;
                new_source = Some(module.source);
            }

            if let Some(error) = lexer.take_error() {
                return Err(From::from(error));
            }

            deps.into_iter().map(|s| s.into_owned()).collect()
        };

        // Convert hashbang #! to //
        if new_source.as_ref().unwrap_or(&source).starts_with("#!") {
            if new_source.is_none() {
                new_source = Some(source.clone())
            }
            new_source.as_mut().unwrap().replace_range(0..2, "//")
        }

        Ok(ModuleInfo {
            source: match new_source {
                None => modules::Source {
                    prefix,
                    body: source,
                    suffix,
                    original: None,
                },
                Some(new_source) => modules::Source {
                    prefix,
                    body: new_source,
                    suffix,
                    original: Some(source),
                },
            },
            deps,
        })
    }

    fn get_work(&mut self) -> Option<Work> {
        loop {
            match self.queue.pop() {
                Some(work) => return Some(work),
                None => {
                    if self.quit.load(Ordering::Relaxed) {
                        return None;
                    } else {
                        thread::yield_now();
                        // thread::sleep(time::Duration::from_millis(1));
                    }
                }
            }
        }
    }
}
