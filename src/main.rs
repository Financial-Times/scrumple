#![cfg_attr(all(test, feature = "bench"), feature(test))]

use esparse::lex::{self};
use fnv::FnvHashSet;
use lazy_static::lazy_static;
use notify::Watcher;
use regex::Regex;
use std::any::Any;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use std::{env, fmt, io, iter, process, str, string, thread, time};

mod bundler;
mod es6;
mod input_options;
mod manifest;
mod modules;
mod opts;
mod path_ext;
mod resolver;
mod source_maps;
mod vlq;
mod worker;
mod writer;

use bundler::bundle;
use input_options::{InputOptions, PackageManager};
use resolver::{Resolved, Resolver};
use source_maps::SourceMapOutput;

const CORE_MODULES: &[&str] = &[
    "assert",
    "buffer",
    "child_process",
    "cluster",
    "crypto",
    "dgram",
    "dns",
    "domain",
    "events",
    "fs",
    "http",
    "https",
    "net",
    "os",
    "path",
    "punycode",
    "querystring",
    "readline",
    "stream",
    "string_decoder",
    "tls",
    "tty",
    "url",
    "util",
    "v8",
    "vm",
    "zlib",
];

pub fn count_lines(source: &str) -> usize {
    // TODO non-ASCII line terminators?
    1 + memchr::Memchr::new(b'\n', source.as_bytes()).count()
}

pub fn to_quoted_json_string(s: &str) -> String {
    // Serializing to a String only fails if the Serialize impl decides to fail, which the Serialize impl of `str` never does.
    serde_json::to_string(s).unwrap()
}

fn run() -> Result<(), CliError> {
    let entry_inst = time::Instant::now();

    let mut input = None;
    let mut output = None;
    let mut map = None;
    let mut package_manager = PackageManager::Npm;
    let mut map_inline = false;
    let mut no_map = false;
    let mut watch = false;
    let mut quiet_watch = false;
    let mut external = FnvHashSet::default();

    // TODO replace this arg parser
    let mut iter = opts::args();
    while let Some(arg) = iter.next() {
        let opt = match arg {
            opts::Arg::Pos(arg) => {
                if input.is_none() {
                    input = Some(arg)
                } else if output.is_none() {
                    output = Some(arg)
                } else {
                    return Err(CliError::UnexpectedArg(arg));
                }
                continue;
            }
            opts::Arg::Opt(opt) => opt,
        };
        match &*opt {
            "-h" | "--help" => return Err(CliError::Help),
            "-v" | "--version" => return Err(CliError::Version),
            "-w" | "--watch" => watch = true,
            "-W" | "--quiet-watch" => {
                watch = true;
                quiet_watch = true;
            }
            "-I" | "--map-inline" => map_inline = true,
            "-M" | "--no-map" => no_map = true,
            "-b" | "--for-bower" => package_manager = PackageManager::Bower,
            "-x" | "--external" => {
                lazy_static! {
                    static ref COMMA: Regex = Regex::new(r#"\s*,\s*"#).unwrap();
                }
                let mods = iter
                    .next_arg()
                    .ok_or_else(|| CliError::MissingOptionValue(opt))?;
                for m in COMMA.split(&mods) {
                    external.insert(m.to_string());
                }
            }
            "--external-core" => {
                for m in CORE_MODULES {
                    external.insert(m.to_string());
                }
            }
            "-m" | "--map" => {
                if map.is_some() {
                    return Err(CliError::DuplicateOption(opt));
                }
                map = Some(
                    iter.next_arg()
                        .ok_or_else(|| CliError::MissingOptionValue(opt))?,
                )
            }
            "-i" | "--input" => {
                if input.is_some() {
                    return Err(CliError::DuplicateOption(opt));
                }
                input = Some(
                    iter.next_arg()
                        .ok_or_else(|| CliError::MissingOptionValue(opt))?,
                )
            }
            "-o" | "--output" => {
                if output.is_some() {
                    return Err(CliError::DuplicateOption(opt));
                }
                output = Some(
                    iter.next_arg()
                        .ok_or_else(|| CliError::MissingOptionValue(opt))?,
                )
            }
            _ => return Err(CliError::UnknownOption(opt)),
        }
    }

    if map_inline as u8 + no_map as u8 + map.is_some() as u8 > 1 {
        return Err(CliError::BadUsage(
            "--map-inline, --map <file>, and --no-map are mutually exclusive",
        ));
    }

    let input = input.ok_or(CliError::MissingFileName)?;
    let input_dir = env::current_dir()?;
    let output = output.unwrap_or_else(|| "-".to_owned());

    let map_output = if map_inline {
        SourceMapOutput::Inline
    } else if no_map {
        SourceMapOutput::Suppressed
    } else {
        match map {
            Some(path) => SourceMapOutput::File(PathBuf::from(path), Path::new(&output)),
            None => {
                if output == "-" {
                    SourceMapOutput::Suppressed
                } else {
                    let mut buf = OsString::from(&output);
                    buf.push(".map");
                    SourceMapOutput::File(PathBuf::from(buf), Path::new(&output))
                }
            }
        }
    };

    let input_options = InputOptions {
        package_manager,
        external,
    };

    let entry_point = match Resolver::new(input_options.clone()).resolve_main(input_dir, &input)? {
        Resolved::External => return Err(CliError::ExternalMain),
        Resolved::Ignore => return Err(CliError::IgnoredMain),
        Resolved::Normal(path) => path,
    };

    if watch {
        let progress_line = format!(" build {output} ...", output = output);
        eprint!("{}", progress_line);
        io::Write::flush(&mut io::stderr())?;

        let mut modules = match bundle(&entry_point, input_options.clone(), &output, &map_output) {
            Ok(mods) => mods,
            Err(e) => {
                eprintln!();
                return Err(e);
            }
        };
        let elapsed = entry_inst.elapsed();
        let ms = elapsed.as_secs() * 1_000 + u64::from(elapsed.subsec_millis());

        let (tx, rx) = mpsc::channel();
        let debounce_duration = time::Duration::from_millis(5);
        let mut watcher = notify::raw_watcher(tx.clone())?;

        for path in modules.keys() {
            watcher.watch(path, notify::RecursiveMode::NonRecursive)?;
        }

        eprintln!(
            "{bs} ready {output} in {ms} ms",
            output = output,
            ms = ms,
            bs = "\u{8}".repeat(progress_line.len())
        );

        loop {
            let first_event = rx.recv().expect("notify::watcher disconnected");
            thread::sleep(debounce_duration);
            for event in iter::once(first_event).chain(rx.try_iter()) {
                let _op = event.op?;
            }

            eprint!("update {} ...", output);
            io::Write::flush(&mut io::stderr())?;
            let start_inst = time::Instant::now();
            match bundle(&entry_point, input_options.clone(), &output, &map_output) {
                Ok(new_modules) => {
                    let elapsed = start_inst.elapsed();
                    let ms = elapsed.as_secs() * 1_000 + u64::from(elapsed.subsec_millis());
                    eprintln!("{bs}in {ms} ms", ms = ms, bs = "\u{8}".repeat(3));

                    {
                        let mut to_unwatch = modules.keys().collect::<FnvHashSet<_>>();
                        let mut to_watch = new_modules.keys().collect::<FnvHashSet<_>>();
                        for path in modules.keys() {
                            to_watch.remove(&path);
                        }
                        for path in new_modules.keys() {
                            to_unwatch.remove(&path);
                        }
                        for path in to_watch {
                            watcher.watch(path, notify::RecursiveMode::NonRecursive)?;
                        }
                        for path in to_unwatch {
                            watcher.unwatch(path)?;
                        }
                    }
                    modules = new_modules;
                }
                Err(kind) => {
                    eprintln!("{}error: {}", if quiet_watch { "" } else { "\x07" }, kind);
                }
            }
        }
    } else {
        bundle(&entry_point, input_options, &output, &map_output).map(|_| ())
    }
}

const APP_NAME: &str = env!("CARGO_PKG_NAME");
const EXE_NAME: &str = "oax";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

fn write_usage(f: &mut fmt::Formatter) -> fmt::Result {
    write!(
        f,
        "\
Usage: {0} [options] <input> [output]
       {0} [-h | --help | -v | --version]",
        EXE_NAME
    )
}

fn write_version(f: &mut fmt::Formatter) -> fmt::Result {
    write!(f, "{0} v{1}", APP_NAME, APP_VERSION)
}

fn write_help(f: &mut fmt::Formatter) -> fmt::Result {
    write_version(f)?;
    write!(f, "\n\n")?;
    write_usage(f)?;
    write!(f, "\n\n")?;
    writeln!(
        f,
        "\
Options:
    -i, --input <input>
        Use <input> as the main module.

    -o, --output <output>
        Write bundle to <output> and source map to <output>.map.
        Default: '-' for stdout.

    -m, --map <map>
        Output source map to <map>.

    -I, --map-inline
        Output source map inline as data: URI.

    -M, --no-map
        Suppress source map output when it would normally be implied.

    -w, --watch
        Watch for changes to <input> and its dependencies.

    -W, --quiet-watch
        Don't emit a bell character for errors that occur while watching.
        Implies --watch.

    -e, --es-syntax
        Support .mjs files with ECMAScript module syntax:

            import itt from 'itt'
            export const greeting = 'Hello, world!'

        Instead of CommonJS require syntax:

            const itt = require('itt')
            exports.greeting = 'Hello, world!'

        .mjs (ESM) files can import .js (CJS) files, in which case the
        namespace object has a single `default` binding which reflects the
        value of `module.exports`. CJS files can require ESM files, in which
        case the resultant object is the namespace object.

    -E, --es-syntax-everywhere
        Implies --es-syntax. Allow ECMAScript module syntax in .js files.
        CJS-style `require()` calls are also allowed.

    -x, --external <module1,module2,...>
        Don't resolve or include modules named <module1>, <module2>, etc.;
        leave them as require('<module>') references in the bundle. Specifying
        a path instead of a module name does nothing.

    --external-core
        Ignore references to node.js core modules like 'events' and leave them
        as require('<module>') references in the bundle.

    -b, --for-bower
        Use .bower.json instead of package.json

    -h, --help
        Print this message.

    -v, --version
        Print version information."
    )
}

#[derive(Debug)]
pub enum CliError {
    Help,
    Version,
    MissingFileName,
    ExternalMain,
    IgnoredMain,
    DuplicateOption(String),
    MissingOptionValue(String),
    UnknownOption(String),
    UnexpectedArg(String),
    BadUsage(&'static str),

    RequireRoot {
        context: Option<PathBuf>,
        path: PathBuf,
    },
    EmptyModuleName {
        context: PathBuf,
    },
    ModuleNotFound {
        context: PathBuf,
        name: String,
    },
    MainNotFound {
        name: String,
    },

    InvalidUtf8 {
        context: PathBuf,
        err: string::FromUtf8Error,
    },

    Io(io::Error),
    Json(serde_json::Error),
    Notify(notify::Error),
    Es6(es6::error::Error),
    Lex(lex::Error),
    ParseStrLit(lex::ParseStrLitError),
    Box(Box<dyn Any + Send + 'static>),
}
impl From<io::Error> for CliError {
    fn from(inner: io::Error) -> CliError {
        CliError::Io(inner)
    }
}
impl From<serde_json::Error> for CliError {
    fn from(inner: serde_json::Error) -> CliError {
        CliError::Json(inner)
    }
}
impl From<notify::Error> for CliError {
    fn from(inner: notify::Error) -> CliError {
        CliError::Notify(inner)
    }
}
impl From<es6::error::Error> for CliError {
    fn from(inner: es6::error::Error) -> CliError {
        CliError::Es6(inner)
    }
}
impl From<lex::Error> for CliError {
    fn from(inner: lex::Error) -> CliError {
        CliError::Lex(inner)
    }
}
impl From<lex::ParseStrLitError> for CliError {
    fn from(inner: lex::ParseStrLitError) -> CliError {
        CliError::ParseStrLit(inner)
    }
}
impl From<Box<dyn Any + Send + 'static>> for CliError {
    fn from(inner: Box<dyn Any + Send + 'static>) -> CliError {
        CliError::Box(inner)
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            CliError::Help => write_help(f),
            CliError::Version => write_version(f),
            CliError::MissingFileName => write_usage(f),
            CliError::ExternalMain => write!(f, "main module is --external"),
            CliError::IgnoredMain => {
                write!(f, "main module is ignored by a browser field substitution")
            }
            CliError::DuplicateOption(ref opt) => {
                write!(f, "option {} specified more than once", opt)
            }
            CliError::MissingOptionValue(ref opt) => write!(f, "missing value for option {}", opt),
            CliError::UnknownOption(ref opt) => write!(f, "unknown option {}", opt),
            CliError::UnexpectedArg(ref arg) => write!(f, "unexpected argument {}", arg),
            CliError::BadUsage(ref arg) => write!(f, "{}", arg),

            CliError::RequireRoot {
                ref context,
                ref path,
            } => match *context {
                None => write!(f, "main module is root path {}", path.display(),),
                Some(ref context) => write!(
                    f,
                    "require of root path {} in {}",
                    path.display(),
                    context.display(),
                ),
            },
            CliError::EmptyModuleName { ref context } => {
                write!(f, "require('') in {}", context.display())
            }
            CliError::ModuleNotFound {
                ref context,
                ref name,
            } => write!(f, "module '{}' not found in {}", name, context.display(),),
            CliError::MainNotFound { ref name } => write!(f, "main module '{}' not found", name),

            CliError::InvalidUtf8 {
                ref context,
                ref err,
            } => write!(f, "in {}: {}", context.display(), err),

            CliError::Io(ref inner) => write!(f, "{}", inner),
            CliError::Json(ref inner) => write!(f, "{}", inner),
            CliError::Notify(ref inner) => write!(f, "{}", inner),
            CliError::Es6(ref inner) => write!(f, "{}", inner),
            CliError::Lex(ref inner) => write!(f, "{}", inner),
            CliError::ParseStrLit(ref inner) => write!(f, "{}", inner),
            CliError::Box(ref inner) => write!(f, "{:?}", inner),
        }
    }
}

fn main() {
    process::exit(match run() {
        Ok(_) => 0,
        Err(kind) => {
            match kind {
                CliError::Help | CliError::Version | CliError::MissingFileName => {
                    println!("{}", kind);
                }
                _ => {
                    println!("{}: {}", EXE_NAME, kind);
                }
            }
            1
        }
    })
}

#[cfg(test)]
mod test;
