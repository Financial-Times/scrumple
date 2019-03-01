#![cfg_attr(all(test, feature = "bench"), feature(test))]

#[macro_use]
extern crate esparse;
extern crate crossbeam;
extern crate num_cpus;
extern crate notify;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate memchr;
extern crate base64;
extern crate regex;
extern crate fnv;
#[macro_use]
extern crate matches;
#[macro_use]
extern crate lazy_static;

#[cfg(test)]
#[macro_use]
extern crate cfg_if;
#[cfg(test)]
#[macro_use]
extern crate indoc;
#[cfg(test)]
extern crate tempfile;
#[cfg(test)]
extern crate walkdir;

use std::{env, process, io, fs, thread, time, iter, fmt, str, string, mem};
use std::io::prelude::*;
use std::fmt::{Display, Write};
use std::path::{self, PathBuf, Path, Component};
use std::sync::mpsc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::marker::PhantomData;
use std::sync::Arc;
use std::rc::Rc;
use std::any::Any;
use std::borrow::Cow;
use std::cell::RefCell;
use std::ffi::OsString;
use fnv::{FnvHashMap, FnvHashSet};
use crossbeam::sync::SegQueue;
use notify::Watcher;
use esparse::lex::{self, Tt};
use serde::de::{self, Deserialize, Deserializer, Visitor};
use serde::ser::{Serialize, Serializer, SerializeSeq};
use regex::Regex;

mod opts;
mod es6;

macro_rules! map {
    {} => {
        Default::default()
    };
    { $($key:expr => $value:expr,)+ } => {
        {
            let mut map = FnvHashMap::default();
            $(map.insert($key, $value);)+
            map
        }
    };
    { $($key:expr => $value:expr),+ } => {
        map!{$($key => $value,)+}
    };
}

const HEAD_JS: &str = include_str!("head.js");
const TAIL_JS: &str = include_str!("tail.js");
const CORE_MODULES: &[&str] = &["assert", "buffer", "child_process", "cluster", "crypto", "dgram", "dns", "domain", "events", "fs", "http", "https", "net", "os", "path", "punycode", "querystring", "readline", "stream", "string_decoder", "tls", "tty", "url", "util", "v8", "vm", "zlib"];

fn cjs_parse_deps<'f, 's>(lex: &mut lex::Lexer<'f, 's>) -> Result<FnvHashSet<Cow<'s, str>>, CliError> {
    // TODO should we panic on dynamic requires?
    let mut deps = FnvHashSet::default();
    loop {
        eat!(lex,
            // Tt::Id(s) if s == "require" => eat!(lex,
            Tt::Id("require") => eat!(lex,
                Tt::Lparen => eat!(lex,
                    Tt::StrLitSgl(s) |
                    Tt::StrLitDbl(s) => eat!(lex,
                        Tt::Rparen => {
                            deps.insert(lex::str_lit_value(s)?);
                        },
                        _ => {},
                    ),
                    _ => {},
                ),
                // Tt::Dot => eat!(lex,
                //     Tt::Id("resolve") => eat!(lex,
                //         Tt::Lparen => eat!(lex,
                //             Tt::StrLitSgl(s) |
                //             Tt::StrLitDbl(s) => eat!(lex,
                //                 Tt::Rparen => {
                //                     // TODO handle error
                //                     return Some(lex::str_lit_value(s).unwrap())
                //                 },
                //                 _ => {},
                //             ),
                //             _ => {},
                //         ),
                //         _ => {},
                //     ),
                //     _ => {},
                // ),
                _ => {},
            ),
            Tt::Eof => return Ok(deps),
            _ => {
                lex.advance();
            },
        );
    }
}

#[derive(Debug)]
struct Writer<'a, 'b> {
    modules: FnvHashMap<PathBuf, Module>,
    entry_point: &'a Path,
    map_output: &'b SourceMapOutput<'b>,
}

impl<'a, 'b> Writer<'a, 'b> {
    fn sorted_modules(&self) -> Vec<(&Path, &Module)> {
        let mut modules = self.modules
            .iter()
            .map(|(p, m)| (p.as_path(), m))
            .collect::<Vec<_>>();
        modules.sort_by(|(f, _), (g, _)| f.cmp(g));
        modules
    }

    fn write_to<W: io::Write>(&self, w: &mut W) -> io::Result<()> {
        w.write_all(HEAD_JS.as_bytes())?;
        // for (module, main) in self.mains {
        //     write!(w,
        //         "\n  Pax.mains[{mod_path}] = {main_path}",
        //         mod_path = Self::js_path(&module),
        //         main_path = Self::js_path(&main),
        //     );
        // }

        for (file, info) in self.sorted_modules() {
            let id = Self::name_path(&file);
            let deps = Self::stringify_deps(&info.deps);
            let filename = Self::js_path(&file);

            write!(w,
                "\n  Pax.files[{filename}] = {id}; {id}.deps = {deps}; {id}.filename = {filename}; function {id}(module, exports, require, __filename, __dirname, __import_meta) {{\n",
                filename = filename,
                id = id,
                deps = deps,
            )?;
            if !info.source.prefix.is_empty() {
                w.write_all(info.source.prefix.as_bytes())?;
                w.write_all(b"\n")?;
            }
            w.write_all(info.source.body.as_bytes())?;
            if !matches!(info.source.body.chars().last(), None | Some('\n') | Some('\r') | Some('\u{2028}') | Some('\u{2029}')) {
                w.write_all(b"\n")?;
            }
            if !info.source.suffix.is_empty() {
                w.write_all(info.source.suffix.as_bytes())?;
            }
            write!(w, "}}")?;
        }
        let main = Self::name_path(self.entry_point);
        write!(w,
            "\n  Pax.main = {main}; Pax.makeRequire(null)()\n  if (typeof module !== 'undefined') module.exports = Pax.main.module && Pax.main.module.exports\n",
            main = main,
        )?;
        w.write_all(TAIL_JS.as_bytes())?;
        match *self.map_output {
            SourceMapOutput::Suppressed => {}
            SourceMapOutput::Inline => {
                let mut map = Vec::new();
                self.write_map_to(&mut map)?;
                write!(w,
                    "//# sourceMappingURL=data:application/json;charset=utf-8;base64,{data}\n",
                    data = base64::encode(&map),
                )?;
            }
            SourceMapOutput::File(ref path, output_file) => {
                // TODO handle error
                let relative = path.relative_from(output_file.parent().unwrap());
                let map = relative.as_ref().unwrap_or(path);
                write!(w,
                    "//# sourceMappingURL={map}\n",
                    map = map.display(),
                )?;
            }
        }
        Ok(())
    }

    fn write_map_to<W: io::Write>(&self, w: &mut W) -> serde_json::Result<()> {
        // https://sourcemaps.info/spec.html

        let ref modules = self.sorted_modules();
        let dir = self.entry_point.parent().unwrap();

        #[derive(Serialize, Debug)]
        #[serde(rename_all = "camelCase")]
        struct SourceMap<'a> {
            version: u8,
            file: &'static str,
            source_root: &'static str,
            sources: Sources<'a>,
            sources_content: SourcesContent<'a>,
            names: [(); 0],
            mappings: Mappings<'a>,
        }

        #[derive(Debug)]
        struct Sources<'a> {
            modules: &'a [(&'a Path, &'a Module)],
            dir: &'a Path,
        }

        impl<'a> Serialize for Sources<'a> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let mut seq = serializer.serialize_seq(None)?;
                for (f, _) in self.modules {
                    let relative = f.relative_from(self.dir);
                    let path = relative.as_ref().map_or(*f, PathBuf::as_path);
                    seq.serialize_element(&path.to_string_lossy())?;
                }
                seq.end()
            }
        }

        #[derive(Debug)]
        struct SourcesContent<'a> {
            modules: &'a [(&'a Path, &'a Module)],
        }

        impl<'a> Serialize for SourcesContent<'a> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let mut seq = serializer.serialize_seq(None)?;
                for (_, module) in self.modules {
                    let content = module.source.original.as_ref().unwrap_or(&module.source.body);
                    seq.serialize_element(content)?;
                }
                seq.end()
            }
        }

        #[derive(Debug)]
        struct Mappings<'a> {
            modules: &'a [(&'a Path, &'a Module)],
        }

        impl<'a> Serialize for Mappings<'a> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.collect_str(self)
            }
        }

        impl<'a> Display for Mappings<'a> {
            fn fmt(&self, w: &mut fmt::Formatter) -> fmt::Result {
                let prefix_len = count_lines(HEAD_JS); /*+ this.mains.size*/
                for _ in 0..prefix_len {
                    w.write_str(";")?;
                }

                let mut line = 0;
                let mut vlq = Vlq::new();
                for (index, &(_, module)) in self.modules.iter().enumerate() {
                    w.write_str(";")?;
                    if !module.source.prefix.is_empty() {
                        for _ in 0..count_lines(&module.source.prefix) {
                            w.write_str(";")?;
                        }
                    }
                    for i in 0..count_lines(&module.source.body) {
                        w.write_str("A")?;
                        if i == 0 {
                            if index == 0 {
                                w.write_str("AAA")?;
                            } else {
                                w.write_str("C")?;
                                w.write_str(vlq.enc(-line))?;
                                w.write_str("A")?;
                            }
                            line = 0;
                        } else {
                            w.write_str("ACA")?;
                            line += 1;
                        }
                        w.write_str(";")?;
                    }
                    if !matches!(module.source.body.chars().last(), None | Some('\n') | Some('\r') | Some('\u{2028}') | Some('\u{2029}')) {
                        w.write_str(";")?;
                    }
                    for _ in 0..count_lines(&module.source.suffix)-1 {
                        w.write_str(";")?;
                    }
                }
                for _ in 0..2 + count_lines(TAIL_JS) + 1 - 1 - 1 {
                    w.write_str(";")?;
                }
                Ok(())
            }
        }

        serde_json::to_writer(w, &SourceMap {
            version: 3,
            file: "",
            source_root: "",
            sources: Sources { modules, dir },
            sources_content: SourcesContent { modules },
            names: [],
            mappings: Mappings { modules },
        })
    }

    fn stringify_deps(deps: &FnvHashMap<String, Resolved>) -> String {
        let mut result = "{".to_owned();
        let mut comma = false;
        for (name, resolved) in deps {
            match *resolved {
                Resolved::External => {}
                Resolved::Ignore => {
                    if comma {
                        result.push(',');
                    }
                    result.push_str(&to_quoted_json_string(name));
                    result.push_str(":Pax.ignored");
                    comma = true;
                }
                Resolved::Normal(ref path) => {
                    if comma {
                        result.push(',');
                    }
                    result.push_str(&to_quoted_json_string(name));
                    result.push(':');
                    Self::write_name_path(path, &mut result);
                    comma = true;
                }
            }
        }
        result.push('}');
        result
    }

    #[cfg(target_os = "windows")]
    fn js_path(path: &Path) -> String {
        // TODO untested
        let string = path.to_string_lossy();
        let replaced = string.replace('\\', "/");
        to_quoted_json_string(&replaced)
    }

    #[cfg(not(target_os = "windows"))]
    fn js_path(path: &Path) -> String {
        let string = path.to_string_lossy();
        to_quoted_json_string(&string)
    }

    fn name_path(path: &Path) -> String {
        let mut result = String::new();
        Self::write_name_path(path, &mut result);
        result
    }
    fn write_name_path(path: &Path, result: &mut String) {
        let string = path.to_string_lossy();
        // let slice = string.as_ref();
        let bytes = string.as_bytes();

        result.push_str("file_");
        for &b in bytes {
            match b {
                b'_' | b'a'...b'z' | b'A'...b'Z' | b'0'...b'9' => {
                    result.push(b as char);
                }
                _ => {
                    write!(result, "${:02x}", b).unwrap();
                }
            }
        }

        // let mut last_pos = 0;
        // for pos in bytes.match_indices(|&b| {
        //     !matches!(b as char, '_' | 'a'...'z' | 'A'...'Z')
        // }) {
        //     result.push_str(slice[last_pos..pos]);
        //     write!(result, "${:02x}", bytes[pos]);
        //     last_pos = pos + 1;
        // }
    }
}

fn to_quoted_json_string(s: &str) -> String {
    // Serializing to a String only fails if the Serialize impl decides to fail, which the Serialize impl of `str` never does.
    serde_json::to_string(s).unwrap()
}

fn count_lines(source: &str) -> usize {
    // TODO non-ASCII line terminators?
    1 + memchr::Memchr::new(b'\n', source.as_bytes()).count()
}

struct Vlq {
    buf: [u8; 13],
}
impl Vlq {
    fn new() -> Self {
        Self {
            buf: [0u8; 13],
        }
    }

    fn enc(&mut self, n: isize) -> &str {
        let sign = n < 0;
        let n = if sign { -n } else { n } as usize;
        let mut y = (n & 0xf) << 1 | sign as usize;
        let mut r = n >> 4;
        let mut l = 0;
        while r > 0 {
            y |= 0x20;
            self.buf[l] = B64[y];
            y = r & 0x1f;
            r >>= 5;
            l += 1;
        }
        self.buf[l] = B64[y];
        str::from_utf8(&self.buf[0..l+1]).unwrap()
    }
}
const B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

#[derive(Debug, Clone)]
struct WorkerInit {
    tx: mpsc::Sender<Result<WorkDone, CliError>>,
    input_options: InputOptions,
    queue: Arc<SegQueue<Work>>,
    quit: Arc<AtomicBool>,
}
#[derive(Debug, Clone)]
struct Worker {
    tx: mpsc::Sender<Result<WorkDone, CliError>>,
    resolver: Resolver,
    queue: Arc<SegQueue<Work>>,
    quit: Arc<AtomicBool>,
}

#[derive(Debug, Clone, Default)]
struct Resolver {
    input_options: InputOptions,
    cache: PackageCache,
}

#[derive(Debug, Clone, Default)]
struct PackageCache {
    pkgs: RefCell<FnvHashMap<PathBuf, Option<Rc<PackageInfo>>>>,
}

#[derive(Debug)]
enum Work {
    Resolve { context: PathBuf, name: String },
    Include { module: PathBuf },
}
#[derive(Debug)]
enum WorkDone {
    Resolve { context: PathBuf, name: String, resolved: Resolved },
    Include { module: PathBuf, info: ModuleInfo },
}
#[derive(Debug)]
enum ModuleState {
    Loading,
    Loaded(Module),
}
#[derive(Debug)]
pub struct Module {
    pub source: Source,
    pub deps: FnvHashMap<String, Resolved>,
}
#[derive(Debug)]
struct ModuleInfo {
    source: Source,
    deps: Vec<String>,
}
#[derive(Debug)]
pub struct Source {
    pub prefix: String,
    pub body: String,
    pub suffix: String,
    pub original: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    External,
    Ignore,
    // CoreWithSubst(PathBuf),
    Normal(PathBuf),
}
#[derive(Debug, Clone, PartialEq, Eq)]
enum ModuleSubstitution {
    Normal,
    Ignore,
    External,
    Replace(String),
}
#[derive(Debug, Clone, PartialEq, Eq)]
enum PathSubstitution {
    Missing,
    Normal,
    Ignore,
    Replace(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InputOptions {
    pub for_browser: bool,
    pub es6_syntax: bool,
    pub es6_syntax_everywhere: bool,
    pub external: FnvHashSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceMapOutput<'a> {
    Suppressed,
    Inline,
    File(PathBuf, &'a Path),
}

impl ModuleState {
    fn expect(self, message: &str) -> Module {
        match self {
            ModuleState::Loading => panic!("{}", message),
            ModuleState::Loaded(module) => module,
        }
    }
    fn unwrap(self) -> Module {
        self.expect("unwrapped ModuleState that was still loading")
    }
}

pub fn bundle(entry_point: &Path, input_options: InputOptions, output: &str, map_output: &SourceMapOutput) -> Result<FnvHashMap<PathBuf, Module>, CliError> {
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

    worker_init.add_work(Work::Include { module: entry_point.to_owned() });
    pending += 1;
    modules.insert(entry_point.to_owned(), ModuleState::Loading);

    let children: Vec<_> = (0..thread_count).map(|_| {
        let init = worker_init.clone();
        thread::spawn(move || Worker::new(init).run())
    }).collect();
    // let children: Vec<_> = (0..thread_count).map(|n| {
    //     let init = worker_init.clone();
    //     thread::Builder::new().name(format!("worker #{}", n + 1)).spawn(move || Worker::new(init).run()).unwrap()
    // }).collect();

    while let Ok(work_done) = rx.recv() {
        // eprintln!("{:?}", work_done);
        let work_done = match work_done {
            Err(error) => {
                worker_init.quit.store(true, Ordering::Relaxed);
                return Err(error)
            }
            Ok(work_done) => {
                pending -= 1;
                work_done
            }
        };
        match work_done {
            WorkDone::Resolve { context, name, resolved } => {
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
                let old = modules.insert(module.clone(), ModuleState::Loaded(Module {
                    source: info.source,
                    deps: FnvHashMap::default(),
                }));
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
            break
        }
    }

    worker_init.quit.store(true, Ordering::Relaxed);
    for child in children {
        child.join()?;
    }

    let writer = Writer {
        modules: modules.into_iter()
        .map(|(k, ms)| (k, ms.unwrap()))
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

fn run() -> Result<(), CliError> {
    let entry_inst = time::Instant::now();

    let mut input = None;
    let mut output = None;
    let mut map = None;
    let mut for_browser = false;
    let mut es6_syntax = false;
    let mut es6_syntax_everywhere = false;
    let mut map_inline = false;
    let mut no_map = false;
    let mut watch = false;
    let mut quiet_watch = false;
    let mut external = FnvHashSet::default();

    let mut iter = opts::args();
    while let Some(arg) = iter.next() {
        let opt = match arg {
            opts::Arg::Pos(arg) => {
                if input.is_none() {
                    input = Some(arg)
                } else if output.is_none() {
                    output = Some(arg)
                } else {
                    return Err(CliError::UnexpectedArg(arg))
                }
                continue
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
            },
            "-I" | "--map-inline" => map_inline = true,
            "-M" | "--no-map" => no_map = true,
            "-b" | "--for-browser" => for_browser = true,
            "-e" | "--es-syntax" => es6_syntax = true,
            "-E" | "--es-syntax-everywhere" => {
                es6_syntax = true;
                es6_syntax_everywhere = true;
            }
            "-x" | "--external" => {
                lazy_static! {
                    static ref COMMA: Regex = Regex::new(r#"\s*,\s*"#).unwrap();
                }
                let mods = iter.next_arg().ok_or_else(|| CliError::MissingOptionValue(opt))?;
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
                    return Err(CliError::DuplicateOption(opt))
                }
                map = Some(iter.next_arg().ok_or_else(|| CliError::MissingOptionValue(opt))?)
            }
            "-i" | "--input" => {
                if input.is_some() {
                    return Err(CliError::DuplicateOption(opt))
                }
                input = Some(iter.next_arg().ok_or_else(|| CliError::MissingOptionValue(opt))?)
            }
            "-o" | "--output" => {
                if output.is_some() {
                    return Err(CliError::DuplicateOption(opt))
                }
                output = Some(iter.next_arg().ok_or_else(|| CliError::MissingOptionValue(opt))?)
            }
            _ => {
                return Err(CliError::UnknownOption(opt))
            }
        }
    }

    if map_inline as u8 + no_map as u8 + map.is_some() as u8 > 1 {
        return Err(CliError::BadUsage("--map-inline, --map <file>, and --no-map are mutually exclusive"))
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
            Some(path) => {
                SourceMapOutput::File(PathBuf::from(path), Path::new(&output))
            }
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
        for_browser,
        es6_syntax,
        es6_syntax_everywhere,
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
                return Err(e)
            }
        };
        let elapsed = entry_inst.elapsed();
        let ms = elapsed.as_secs() * 1_000 + u64::from(elapsed.subsec_millis());

        let (tx, rx) = mpsc::channel();
        let debounce_dur = time::Duration::from_millis(5);
        let mut watcher = notify::raw_watcher(tx.clone())?;

        for path in modules.keys() {
            watcher.watch(path, notify::RecursiveMode::NonRecursive)?;
        }

        eprintln!("{bs} ready {output} in {ms} ms", output = output, ms = ms, bs = "\u{8}".repeat(progress_line.len()));

        loop {
            let first_event = rx.recv().expect("notify::watcher disconnected");
            thread::sleep(debounce_dur);
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
const EXE_NAME: &str = "px";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

fn write_usage(f: &mut fmt::Formatter) -> fmt::Result {
    write!(f, "\
Usage: {0} [options] <input> [output]
       {0} [-h | --help | -v | --version]", EXE_NAME)
}

fn write_version(f: &mut fmt::Formatter) -> fmt::Result {
    write!(f, "{0} v{1}", APP_NAME, APP_VERSION)
}

fn write_help(f: &mut fmt::Formatter) -> fmt::Result {
    write_version(f)?;
    write!(f, "\n\n")?;
    write_usage(f)?;
    write!(f, "\n\n")?;
    write!(f, "\
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

    -b, --for-browser
        Perform substitutions specified by the `browser` field in package.json.

        https://github.com/defunctzombie/package-browser-field-spec

    -h, --help
        Print this message.

    -v, --version
        Print version information.
")
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

    RequireRoot { context: Option<PathBuf>, path: PathBuf },
    EmptyModuleName { context: PathBuf },
    ModuleNotFound { context: PathBuf, name: String },
    MainNotFound { name: String },

    InvalidUtf8 { context: PathBuf, err: string::FromUtf8Error },

    Io(io::Error),
    Json(serde_json::Error),
    Notify(notify::Error),
    Es6(es6::Error),
    Lex(lex::Error),
    ParseStrLit(lex::ParseStrLitError),
    Box(Box<Any + Send + 'static>),
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
impl From<es6::Error> for CliError {
    fn from(inner: es6::Error) -> CliError {
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
impl From<Box<Any + Send + 'static>> for CliError {
    fn from(inner: Box<Any + Send + 'static>) -> CliError {
        CliError::Box(inner)
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            CliError::Help => {
                write_help(f)
            }
            CliError::Version => {
                write_version(f)
            }
            CliError::MissingFileName => {
                write_usage(f)
            }
            CliError::ExternalMain => {
                write!(f, "main module is --external")
            }
            CliError::IgnoredMain => {
                write!(f, "main module is ignored by a browser field substitution")
            }
            CliError::DuplicateOption(ref opt) => {
                write!(f, "option {} specified more than once", opt)
            }
            CliError::MissingOptionValue(ref opt) => {
                write!(f, "missing value for option {}", opt)
            }
            CliError::UnknownOption(ref opt) => {
                write!(f, "unknown option {}", opt)
            }
            CliError::UnexpectedArg(ref arg) => {
                write!(f, "unexpected argument {}", arg)
            }
            CliError::BadUsage(ref arg) => {
                write!(f, "{}", arg)
            }

            CliError::RequireRoot { ref context, ref path } => {
                match *context {
                    None => {
                        write!(f,
                            "main module is root path {}",
                            path.display(),
                        )
                    }
                    Some(ref context) => {
                        write!(f,
                            "require of root path {} in {}",
                            path.display(),
                            context.display(),
                        )
                    }
                }
            }
            CliError::EmptyModuleName { ref context } => {
                write!(f, "require('') in {}", context.display())
            }
            CliError::ModuleNotFound { ref context, ref name } => {
                write!(f,
                    "module '{}' not found in {}",
                    name,
                    context.display(),
                )
            }
            CliError::MainNotFound { ref name } => {
                write!(f, "main module '{}' not found", name)
            }

            CliError::InvalidUtf8 { ref context, ref err } => {
                write!(f, "in {}: {}", context.display(), err)
            }

            CliError::Io(ref inner) => {
                write!(f, "{}", inner)
            }
            CliError::Json(ref inner) => {
                write!(f, "{}", inner)
            }
            CliError::Notify(ref inner) => {
                write!(f, "{}", inner)
            }
            CliError::Es6(ref inner) => {
                write!(f, "{}", inner)
            }
            CliError::Lex(ref inner) => {
                write!(f, "{}", inner)
            }
            CliError::ParseStrLit(ref inner) => {
                write!(f, "{}", inner)
            }
            CliError::Box(ref inner) => {
                write!(f, "{:?}", inner)
            }
        }
    }
}

fn main() {
    process::exit(match run() {
        Ok(_) => 0,
        Err(kind) => {
            match kind {
                CliError::Help |
                CliError::Version |
                CliError::MissingFileName => {
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

trait PathBufExt {
    fn append_resolving<P: AsRef<Path> + ?Sized>(&mut self, more: &P);
    fn prepend_resolving<P: AsRef<Path> + ?Sized>(&mut self, base: &P);
    fn clear(&mut self);
    fn replace_with<P: AsRef<Path> + ?Sized>(&mut self, that: &P);
    fn as_mut_vec(&mut self) -> &mut Vec<u8>;
}
impl PathBufExt for PathBuf {
    fn append_resolving<P: AsRef<Path> + ?Sized>(&mut self, more: &P) {
        for c in more.as_ref().components() {
            match c {
                Component::Prefix(prefix) => {
                    *self = PathBuf::from(prefix.as_os_str().to_owned());
                }
                Component::RootDir => {
                    self.push(path::MAIN_SEPARATOR.to_string());
                }
                Component::CurDir => {}
                Component::ParentDir => {
                    self.pop();
                }
                Component::Normal(part) => {
                    self.push(part);
                }
            }
        }
    }
    fn prepend_resolving<P: AsRef<Path> + ?Sized>(&mut self, base: &P) {
        let mut tmp = base.as_ref().to_owned();
        mem::swap(self, &mut tmp);
        self.append_resolving(tmp.as_path());
    }
    fn clear(&mut self) {
        self.as_mut_vec().clear();
    }
    fn replace_with<P: AsRef<Path> + ?Sized>(&mut self, that: &P) {
        self.clear();
        self.push(that);
    }

    fn as_mut_vec(&mut self) -> &mut Vec<u8> {
        unsafe { &mut *(self as *mut PathBuf as *mut Vec<u8>) }
    }
}

trait PathExt {
    fn is_explicitly_relative(&self) -> bool;
    fn relative_from<P: AsRef<Path> + ?Sized>(&self, base: &P) -> Option<PathBuf>;
}
impl PathExt for Path {
    #[inline]
    fn is_explicitly_relative(&self) -> bool {
        matches!(self.components().next(), Some(Component::CurDir) | Some(Component::ParentDir))
    }
    fn relative_from<P: AsRef<Path> + ?Sized>(&self, base: &P) -> Option<PathBuf> {
        let base = base.as_ref();
        if self.is_absolute() != base.is_absolute() {
            if self.is_absolute() {
                Some(PathBuf::from(self))
            } else {
                None
            }
        } else {
            let mut ita = self.components();
            let mut itb = base.components();
            let mut comps: Vec<Component> = vec![];
            loop {
                match (ita.next(), itb.next()) {
                    (None, None) => break,
                    (Some(a), None) => {
                        comps.push(a);
                        comps.extend(ita.by_ref());
                        break;
                    }
                    (None, _) => comps.push(Component::ParentDir),
                    (Some(a), Some(b)) if comps.is_empty() && a == b => (),
                    (Some(a), Some(b)) if b == Component::CurDir => comps.push(a),
                    (Some(_), Some(b)) if b == Component::ParentDir => return None,
                    (Some(a), Some(_)) => {
                        comps.push(Component::ParentDir);
                        for _ in itb {
                            comps.push(Component::ParentDir);
                        }
                        comps.push(a);
                        comps.extend(ita.by_ref());
                        break;
                    }
                }
            }
            Some(comps.iter().map(|c| c.as_os_str()).collect())
        }
    }
}

impl WorkerInit {
    fn add_work(&self, work: Work) {
        self.queue.push(work);
    }
}

impl Worker {
    fn new(init: WorkerInit) -> Self {
        Worker {
            tx: init.tx,
            resolver: Resolver::new(init.input_options),
            queue: init.queue,
            quit: init.quit,
        }
    }

    fn run(mut self) {
        while let Some(work) = self.get_work() {
            let work_done = match work {
                Work::Resolve { context, name } => {
                    self.resolver.resolve(&context, &name)
                    .map(|resolved| WorkDone::Resolve {
                        context,
                        name,
                        resolved,
                    })
                }
                Work::Include { module } => {
                    self.include(&module)
                    .map(|info| WorkDone::Include {
                        module,
                        info,
                    })
                }
            };
            if self.tx.send(work_done).is_err() { return }
        }
    }

    fn include(&self, module: &Path) -> Result<ModuleInfo, CliError> {
        let source = {
            let file = fs::File::open(module)?;
            let mut buf_reader = io::BufReader::new(file);
            let mut bytes = Vec::new();
            buf_reader.read_to_end(&mut bytes)?;
            match String::from_utf8(bytes) {
                Ok(s) => s,
                Err(err) => return Err(CliError::InvalidUtf8 {
                    context: module.to_owned(),
                    err,
                }),
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

            } else if self.resolver.input_options.es6_syntax_everywhere {
                let module = es6::module_to_cjs(&mut lexer, true)?;
                // println!("{:#?}", module);
                deps = module.deps;
                prefix = module.source_prefix;
                suffix = module.source_suffix;
                new_source = Some(module.source);

            } else {
                deps = cjs_parse_deps(&mut lexer)?;
                prefix = String::new();
                suffix = String::new();
            }

            if let Some(error) = lexer.take_error() {
                return Err(From::from(error))
            }

            deps.into_iter()
                .map(|s| s.into_owned())
                .collect()
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
                None => Source {
                    prefix,
                    body: source,
                    suffix,
                    original: None,
                },
                Some(new_source) => Source {
                    prefix,
                    body: new_source,
                    suffix,
                    original: Some(source),
                }
            },
            deps,
        })
    }

    fn get_work(&mut self) -> Option<Work> {
        loop {
            match self.queue.try_pop() {
                Some(work) => return Some(work),
                None => {
                    if self.quit.load(Ordering::Relaxed) {
                        return None
                    } else {
                        thread::yield_now();
                        // thread::sleep(time::Duration::from_millis(1));
                    }
                }
            }
        }
    }
}

impl Resolver {
    fn new(input_options: InputOptions) -> Self {
        Resolver {
            input_options,
            ..Default::default()
        }
    }

    fn resolve_main(&self, mut dir: PathBuf, name: &str) -> Result<Resolved, CliError> {
        dir.append_resolving(Path::new(name));
        self.resolve_path_or_module(None, dir)?.ok_or_else(|| {
            CliError::MainNotFound {
                name: name.to_owned(),
            }
        })
    }

    fn resolve(&self, context: &Path, name: &str) -> Result<Resolved, CliError> {
        if name.is_empty() {
            return Err(CliError::EmptyModuleName {
                context: context.to_owned(),
            })
        }

        let path = Path::new(name);
        if path.is_absolute() {
            Ok(
                self.resolve_path_or_module(Some(context), path.to_owned())?.ok_or_else(|| {
                    CliError::ModuleNotFound {
                        context: context.to_owned(),
                        name: name.to_owned(),
                    }
                })?,
            )
        } else if path.is_explicitly_relative() {
            let mut dir = context.to_owned();
            let did_pop = dir.pop(); // to directory
            debug_assert!(did_pop);
            dir.append_resolving(path);
            Ok(
                self.resolve_path_or_module(Some(context), dir)?.ok_or_else(|| {
                    CliError::ModuleNotFound {
                        context: context.to_owned(),
                        name: name.to_owned(),
                    }
                })?,
            )
        } else {
            match self.module_substitution(context, name)? {
                ModuleSubstitution::Ignore => {
                    return Ok(Resolved::Ignore)
                }
                ModuleSubstitution::External => {
                    return Ok(Resolved::External)
                }
                ModuleSubstitution::Replace(new_name) => {
                    // TODO: detect cycles
                    // eprintln!("module replace {} => {}", name, &new_name);
                    return self.resolve(context, &new_name)
                }
                ModuleSubstitution::Normal => {}
            }

            let mut suffix = PathBuf::from("node_modules");
            suffix.push(name);

            let mut dir = context.to_owned();
            while dir.pop() {
                match dir.file_name() {
                    Some(s) if s == "node_modules" => continue,
                    _ => {}
                }
                let new_path = dir.join(&suffix);
                if let Some(result) = self.resolve_path_or_module(Some(context), new_path)? {
                    return Ok(result)
                }
            }

            Err(CliError::ModuleNotFound {
                context: context.to_owned(),
                name: name.to_owned(),
            })
        }
    }

    fn module_substitution(&self, context: &Path, name: &str) -> Result<ModuleSubstitution, CliError> {
        let module_name = name.split('/').next().unwrap();
        if self.input_options.external.contains(module_name) {
            return Ok(ModuleSubstitution::External)
        }
        if self.input_options.for_browser {
            if let Some(p) = context.parent() {
                if let Some(info) = self.cache.nearest_package_info(p.to_owned())? {
                    match info.browser_substitutions.0.get(Path::new(module_name)) {
                        Some(&BrowserSubstitution::Ignore) => {
                            return Ok(ModuleSubstitution::Ignore)
                        }
                        Some(&BrowserSubstitution::Replace(ref to)) => {
                            let mut new_name = to.to_string_lossy().into_owned();
                            new_name.push_str(&name[module_name.len()..]);
                            return Ok(ModuleSubstitution::Replace(new_name))
                        }
                        None => {}
                    }
                }
            }
        }
        Ok(ModuleSubstitution::Normal)
    }

    fn resolve_path_or_module(&self, context: Option<&Path>, mut path: PathBuf) -> Result<Option<Resolved>, CliError> {
        if let Some(info) = self.cache.package_info(&mut path)? {
            path.replace_with(&info.main);
            if self.input_options.for_browser {
                match info.browser_substitutions.0.get(&path) {
                    Some(BrowserSubstitution::Ignore) => {
                        return Ok(Some(Resolved::Ignore))
                    }
                    Some(BrowserSubstitution::Replace(ref to)) => {
                        path.replace_with(to);
                    }
                    None => {},
                }
            }
        }
        self.resolve_path(context, path)
    }

    fn resolve_path(&self, context: Option<&Path>, mut path: PathBuf) -> Result<Option<Resolved>, CliError> {
        let package_info = if self.input_options.for_browser {
            self.cache.nearest_package_info(path.clone())?
        } else {
            None
        };

        macro_rules! check_path {
            ( $package_info:ident, $path:ident ) => {
                // eprintln!("check {}", $path.display());
                if self.input_options.for_browser {
                    match Self::check_path($package_info.as_ref().map(|x| x.as_ref()), &$path) {
                        PathSubstitution::Normal => {
                            // eprintln!("resolve {}", $path.display());
                            return Ok(Some(Resolved::Normal($path)))
                        }
                        PathSubstitution::Ignore => {
                            return Ok(Some(Resolved::Ignore))
                        }
                        PathSubstitution::Replace(p) => {
                            // eprintln!("path replace {} => {}", $path.display(), p.display());
                            return Ok(Some(Resolved::Normal(p)))
                        }
                        PathSubstitution::Missing => {}
                    }
                } else if path.is_file() {
                    return Ok(Some(Resolved::Normal(path)))
                }
            };
        }

        // <path>
        check_path!(package_info, path);

        let file_name = path.file_name().ok_or_else(|| CliError::RequireRoot {
            context: context.map(|p| p.to_owned()),
            path: path.clone(),
        })?.to_owned();

        if self.input_options.es6_syntax {
            // <path>.mjs
            let mut mjs_file_name = file_name.to_owned();
            mjs_file_name.push(".mjs");
            path.set_file_name(&mjs_file_name);
            check_path!(package_info, path);

            // <path>/index.mjs
            path.set_file_name(&file_name);
            path.push("index.mjs");
            check_path!(package_info, path);
            path.pop();
        }

        // <path>.js
        let mut new_file_name = file_name.to_owned();
        new_file_name.push(".js");
        path.set_file_name(&new_file_name);
        check_path!(package_info, path);

        // <path>/index.js
        path.set_file_name(&file_name);
        path.push("index.js");
        check_path!(package_info, path);
        path.pop();

        // <path>.json
        new_file_name.push("on"); // .js|on
        path.set_file_name(&new_file_name);
        check_path!(package_info, path);

        // <path>/index.json
        path.set_file_name(&file_name);
        path.push("index.json");
        check_path!(package_info, path);
        // path.pop();

        Ok(None)
    }

    fn check_path(package_info: Option<&PackageInfo>, path: &Path) -> PathSubstitution {
        if let Some(package_info) = package_info {
            match package_info.browser_substitutions.0.get(path) {
                Some(BrowserSubstitution::Ignore) => {
                    return PathSubstitution::Ignore
                }
                Some(BrowserSubstitution::Replace(ref path)) => {
                    return PathSubstitution::Replace(path.clone())
                }
                None => {}
            }
        }
        if path.is_file() {
            PathSubstitution::Normal
        } else {
            PathSubstitution::Missing
        }
    }
}

impl PackageCache {
    fn nearest_package_info(&self, mut dir: PathBuf) -> Result<Option<Rc<PackageInfo>>, CliError> {
        loop {
            if !matches!(dir.file_name(), Some(s) if s == "node_modules") {
                if let Some(info) = self.package_info(&mut dir)? {
                    return Ok(Some(info))
                }
            }
            if !dir.pop() { return Ok(None) }
        }
    }
    fn package_info(&self, dir: &mut PathBuf) -> Result<Option<Rc<PackageInfo>>, CliError> {
        let mut pkgs = self.pkgs.borrow_mut();
        Ok(pkgs.entry(dir.clone()).or_insert_with(|| {
            dir.push("package.json");
            if let Ok(file) = fs::File::open(&dir) {
                let buf_reader = io::BufReader::new(file);
                if let Ok(mut info) = serde_json::from_reader::<_, PackageInfo>(buf_reader) {
                    dir.pop();
                    info.set_base(&dir);
                    // eprintln!("info {} {:?}", dir.display(), info);
                    Some(Rc::new(info))
                } else {
                    dir.pop();
                    None
                }
            } else {
                dir.pop();
                None
            }
        }).as_ref().cloned())
    }
}

#[derive(Debug, Default, PartialEq, Eq, Clone)]
struct PackageInfo {
    main: PathBuf,
    browser_substitutions: BrowserSubstitutionMap,
}
impl PackageInfo {
    fn set_base(&mut self, base: &Path) {
        self.main.prepend_resolving(base);
        let substs = mem::replace(&mut self.browser_substitutions, Default::default());
        self.browser_substitutions.0.extend(substs.0.into_iter()
            .map(|(mut from, mut to)| {
                if from.is_explicitly_relative() {
                    from.prepend_resolving(base);
                }
                match to {
                    BrowserSubstitution::Ignore => {}
                    BrowserSubstitution::Replace(ref mut path) => {
                        if path.is_explicitly_relative() {
                            path.prepend_resolving(base);
                        }
                    }
                }
                (from, to)
            }));
    }
}
impl<'de> Deserialize<'de> for PackageInfo {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Default, PartialEq, Eq, Deserialize)]
        #[serde(default)]
        struct RawPackageInfo {
            #[serde(deserialize_with = "from_str_or_none")]
            main: Option<PathBuf>,
            browser: BrowserField,
        }

        let info = RawPackageInfo::deserialize(deserializer)?;
        let main = info.main.unwrap_or(PathBuf::from("./index"));
        let browser_substitutions = info.browser.to_map(&main);
        Ok(PackageInfo {
            main,
            browser_substitutions,
        })
    }
}

macro_rules! visit_unconditionally {
    // bool i64 i128 u64 u128 f64 str bytes none some unit newtype_struct seq map enum
    ($l:lifetime $as:expr) => {};
    ($l:lifetime $as:expr, ) => {};
    ($l:lifetime $as:expr, bool $($x:tt)*) => {
        fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> { Ok($as) }
        visit_unconditionally!($l $as, $($x)*);
    };
    ($l:lifetime $as:expr, i64 $($x:tt)*) => {
        fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> { Ok($as) }
        visit_unconditionally!($l $as, $($x)*);
    };
    ($l:lifetime $as:expr, i128 $($x:tt)*) => {
        fn visit_i128<E: de::Error>(self, _: i128) -> Result<Self::Value, E> { Ok($as) }
        visit_unconditionally!($l $as, $($x)*);
    };
    ($l:lifetime $as:expr, u64 $($x:tt)*) => {
        fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> { Ok($as) }
        visit_unconditionally!($l $as, $($x)*);
    };
    ($l:lifetime $as:expr, u128 $($x:tt)*) => {
        fn visit_u128<E: de::Error>(self, _: u128) -> Result<Self::Value, E> { Ok($as) }
        visit_unconditionally!($l $as, $($x)*);
    };
    ($l:lifetime $as:expr, f64 $($x:tt)*) => {
        fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> { Ok($as) }
        visit_unconditionally!($l $as, $($x)*);
    };
    ($l:lifetime $as:expr, str $($x:tt)*) => {
        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> { Ok($as) }
        visit_unconditionally!($l $as, $($x)*);
    };
    ($l:lifetime $as:expr, bytes $($x:tt)*) => {
        fn visit_bytes<E: de::Error>(self, _: &[u8]) -> Result<Self::Value, E> { Ok($as) }
        visit_unconditionally!($l $as, $($x)*);
    };
    ($l:lifetime $as:expr, none $($x:tt)*) => {
        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> { Ok($as) }
        visit_unconditionally!($l $as, $($x)*);
    };
    ($l:lifetime $as:expr, some $($x:tt)*) => {
        fn visit_some<D: Deserializer<$l>>(self, _: D) -> Result<Self::Value, D::Error> { Ok($as) }
        visit_unconditionally!($l $as, $($x)*);
    };
    ($l:lifetime $as:expr, unit $($x:tt)*) => {
        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> { Ok($as) }
        visit_unconditionally!($l $as, $($x)*);
    };
    ($l:lifetime $as:expr, newtype_struct $($x:tt)*) => {
        fn visit_newtype_struct<D: Deserializer<$l>>(self, _: D) -> Result<Self::Value, D::Error> { Ok($as) }
        visit_unconditionally!($l $as, $($x)*);
    };
    ($l:lifetime $as:expr, seq $($x:tt)*) => {
        fn visit_seq<A: de::SeqAccess<$l>>(self, _: A) -> Result<Self::Value, A::Error> { Ok($as) }
        visit_unconditionally!($l $as, $($x)*);
    };
    ($l:lifetime $as:expr, map $($x:tt)*) => {
        fn visit_map<A: de::MapAccess<$l>>(self, _: A) -> Result<Self::Value, A::Error> { Ok($as) }
        visit_unconditionally!($l $as, $($x)*);
    };
    ($l:lifetime $as:expr, enum $($x:tt)*) => {
        fn visit_enum<A: de::EnumAccess<$l>>(self, _: A) -> Result<Self::Value, A::Error> { Ok($as) }
        visit_unconditionally!($l $as, $($x)*);
    };
}


#[derive(Debug, PartialEq, Eq, Clone)]
enum BrowserField {
    Empty,
    Main(PathBuf),
    Complex(BrowserSubstitutionMap),
}
impl BrowserField {
    fn to_map(self, main: &Path) -> BrowserSubstitutionMap {
        match self {
            BrowserField::Empty => {
                Default::default()
            }
            BrowserField::Main(mut to) => {
                if !to.is_explicitly_relative() {
                    to.prepend_resolving(Path::new("."));
                }
                BrowserSubstitutionMap(map! {
                    main.to_owned() => BrowserSubstitution::Replace(to),
                })
            }
            BrowserField::Complex(map) => map,
        }
    }
}
impl Default for BrowserField {
    fn default() -> Self {
        BrowserField::Empty
    }
}
impl<'de> Deserialize<'de> for BrowserField {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        struct BrowserFieldVisitor;

        impl<'de> Visitor<'de> for BrowserFieldVisitor {
            type Value = BrowserField;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "anything at all")
            }
            fn visit_map<A: de::MapAccess<'de>>(self, access: A) -> Result<Self::Value, A::Error> {
                Ok(BrowserField::Complex(Deserialize::deserialize(de::value::MapAccessDeserializer::new(access))?))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> { Ok(BrowserField::Main(PathBuf::from(v))) }

            visit_unconditionally!('de BrowserField::Empty, bool i64 i128 u64 u128 f64 bytes none some unit newtype_struct seq enum);
        }

        deserializer.deserialize_any(BrowserFieldVisitor)
    }
}

fn from_str_or_none<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where for<'a> T: From<&'a str> + Deserialize<'de>, D: Deserializer<'de> {
    struct FromStrOrNone<T>(PhantomData<T>);

    impl<'de, T> Visitor<'de> for FromStrOrNone<T>
    where for<'a> T: From<&'a str> + Deserialize<'de> {
        type Value = Option<T>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "anything at all")
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            Ok(Some(T::from(v)))
        }

        visit_unconditionally!('de None, bool i64 i128 u64 u128 f64 bytes none some unit newtype_struct seq map enum);
    }

    deserializer.deserialize_any(FromStrOrNone(PhantomData))
}

#[derive(Debug, PartialEq, Eq, Clone)]
enum BrowserSubstitution<T> {
    Ignore,
    Replace(T),
}
#[derive(Debug, Default, Deserialize, PartialEq, Eq, Clone)]
#[serde(transparent)]
struct BrowserSubstitutionMap(FnvHashMap<PathBuf, BrowserSubstitution<PathBuf>>);

impl<'de, T> Deserialize<'de> for BrowserSubstitution<T>
where for<'a> T: From<&'a str> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct SubstitutionVisitor<T>(PhantomData<T>);

        impl<'de, T> Visitor<'de> for SubstitutionVisitor<T>
        where for<'a> T: From<&'a str> {
            type Value = BrowserSubstitution<T>;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "substitution path or false")
            }

            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Self::Value, E> {
                if v {
                    Err(de::Error::invalid_value(de::Unexpected::Bool(v), &self))
                } else {
                    Ok(BrowserSubstitution::Ignore)
                }
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(BrowserSubstitution::Replace(From::from(v)))
            }
        }

        deserializer.deserialize_any(SubstitutionVisitor(PhantomData))
    }
}

#[cfg(test)]
mod test;
