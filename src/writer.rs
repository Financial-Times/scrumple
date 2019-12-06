use crate::modules::Module;
use crate::path_ext::PathExt;
use crate::resolver::Resolved;
use crate::source_maps::SourceMapOutput;
use crate::vlq::Vlq;
use crate::{count_lines, to_quoted_json_string};
use fnv::FnvHashMap;
use matches::matches;
use serde::ser::{SerializeSeq, Serializer};
use serde::Serialize;
use std::fmt::{self, Display, Write};
use std::io;
use std::path::{Path, PathBuf};

const HEAD_JS: &str = include_str!("javascript/head.js");
const TAIL_JS: &str = include_str!("javascript/tail.js");

#[derive(Debug)]
pub struct Writer<'a, 'b> {
    pub modules: FnvHashMap<PathBuf, Module>,
    pub entry_point: &'a Path,
    pub map_output: &'b SourceMapOutput<'b>,
}

impl<'a, 'b> Writer<'a, 'b> {
    fn sorted_modules(&self) -> Vec<(&Path, &Module)> {
        let mut modules = self
            .modules
            .iter()
            .map(|(p, m)| (p.as_path(), m))
            .collect::<Vec<_>>();
        modules.sort_by(|(f, _), (g, _)| f.cmp(g));
        modules
    }

    pub fn write_to<W: io::Write>(&self, w: &mut W) -> io::Result<()> {
        w.write_all(HEAD_JS.as_bytes())?;

        // TODO understand what this was
        // for (module, main) in self.mains {
        //     write!(w,
        //         "\n  Pax.mains[{mod_path}] = {main_path}",
        //         mod_path = Self::js_path(&module),
        //         main_path = Self::js_path(&main),
        //     );
        // }

        for (file, info) in self.sorted_modules() {
            let id = Self::name_path(&file);
            let deps = Self::stringify_deps(&info.deps, PathBuf::from(self.entry_point));
            let filename = Self::js_path(&file);

            // TODO change `Pax.files` to... something Origami or generic
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

            // TODO this should be a function
            let last_char = info.source.body.chars().last();
            // if the last character is not some kind of newline
            if !matches!(
                last_char,
                None
                // newline
                    | Some('\n')
                // carriage_return
                    | Some('\r')
                // line separator
                    | Some('\u{2028}')
                // paragraph separator
                    | Some('\u{2029}')
            ) {
                // then add a newline
                w.write_all(b"\n")?;
            }

            if !info.source.suffix.is_empty() {
                w.write_all(info.source.suffix.as_bytes())?;
            }
            write!(w, "}}")?;
        }
        let main = Self::name_path(
            self.entry_point
                .strip_prefix(self.entry_point.parent().unwrap())
                .unwrap(),
        );
        // TODO put these lines of JS in functions to improve readability
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
                writeln!(
                    w,
                    "//# sourceMappingURL=data:application/json;charset=utf-8;base64,{data}",
                    data = base64::encode(&map),
                )?;
            }
            SourceMapOutput::File(ref path, output_file) => {
                // TODO handle error
                let relative = path.relative_from(output_file.parent().unwrap());
                let map = relative.as_ref().unwrap_or(path);
                writeln!(w, "//# sourceMappingURL={map}", map = map.display(),)?;
            }
        }
        Ok(())
    }

    pub fn write_map_to<W: io::Write>(&self, w: &mut W) -> serde_json::Result<()> {
        // See: https://sourcemaps.info/spec.html

        let modules = &self.sorted_modules();
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
                    let content = module
                        .source
                        .original
                        .as_ref()
                        .unwrap_or(&module.source.body);
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

                    let last_char = module.source.body.chars().last();
                    // if the last character is not some kind of newline
                    if !matches!(
                        last_char,
                        None
                        // newline
                        | Some('\n')
                        // carriage_return
                        | Some('\r')
                        // line separator
                        | Some('\u{2028}')
                        // paragraph separator
                        | Some('\u{2029}')
                    ) {
                        // then add a semi-colon
                        w.write_str(";")?;
                    }

                    // TODO what is module.source.suffix?
                    for _ in 0..count_lines(&module.source.suffix) - 1 {
                        w.write_str(";")?;
                    }
                }
                for _ in 0..2 + count_lines(TAIL_JS) + 1 - 1 - 1 {
                    w.write_str(";")?;
                }
                Ok(())
            }
        }

        serde_json::to_writer(
            w,
            &SourceMap {
                version: 3,
                file: "",
                source_root: "",
                sources: Sources { modules, dir },
                sources_content: SourcesContent { modules },
                names: [],
                mappings: Mappings { modules },
            },
        )
    }

    fn stringify_deps(deps: &FnvHashMap<String, Resolved>, entry_point: PathBuf) -> String {
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
                    let parent = entry_point.parent().unwrap();

                    match path.as_path().strip_prefix(parent) {
                        Ok(path) => Self::write_name_path(path, &mut result),
                        Err(_) => {
                            Self::write_name_path(path, &mut result);
                        }
                    }
                    comma = true;
                }
            }
        }
        result.push('}');
        result
    }

    #[cfg(target_os = "windows")]
    fn js_path(path: &Path) -> String {
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
                b'_' | b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' => {
                    result.push(b as char);
                }
                _ => {
                    write!(result, "${:02x}", b).unwrap();
                }
            }
        }

        // TODO understand what this did
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
