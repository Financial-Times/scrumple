// TODO move unit tests to relevant files (or folders)
#![allow(unused_imports)]

#[cfg(feature = "bench")]
extern crate test;

use super::*;
use fnv::FnvHashMap;
use indoc::indoc;
use input_options::*;
use manifest::{BrowserSubstitution, BrowserSubstitutionMap, PackageInfo};
use matches::assert_matches;
use path_ext::*;
use serde_json;
use std::io::{self, Write};
use std::path::Path;
use std::{ffi, fs, process};
use vlq::Vlq;
use walkdir::WalkDir;

#[test]
fn test_count_lines() {
    assert_eq!(count_lines(""), 1);
    assert_eq!(count_lines("this is a line"), 1);
    assert_eq!(count_lines("this is a line\n"), 2);
    assert_eq!(count_lines("\nthis is a line"), 2);
    assert_eq!(count_lines("\n\n\nthis is a line"), 4);
    assert_eq!(count_lines("this is a line\n\n\n"), 4);
    assert_eq!(count_lines("these\nare\nlines"), 3);
    assert_eq!(count_lines("\r\n"), 2);
    assert_eq!(count_lines("this is a line\r\n"), 2);
    assert_eq!(count_lines("\r\nthis is a line"), 2);
    assert_eq!(count_lines("these\nare\r\nlines"), 3);
}

#[test]
fn test_reproducing() {
    let entry_point_a = Path::new("examples/reproducing/a");
    let entry_point_a = Path::new("examples/reproducing/b");
    npm_install(entry_point.parent().unwrap());
    let options = InputOptions::default();
    let output = "/dev/null";
    let map_output = SourceMapOutput::Inline;
}

use cfg_if::cfg_if;
cfg_if! {
    if #[cfg(feature = "bench")] {
        #[bench]
        fn bench_vlq(b: &mut test::Bencher) {
            let mut vlq = Vlq::new();
            b.iter(|| {
                test::black_box(vlq.enc(-1001));
            });
        }

        #[bench]
        fn bench_cjs_simple(b: &mut test::Bencher) {
            let entry_point = Path::new("examples/simple/index.js");
            npm_install(entry_point.parent().unwrap());
            let options = InputOptions::default();
            let output = "/dev/null";
            let map_output = SourceMapOutput::Inline;

            b.iter(|| {
                let _ = bundle(&entry_point, options, &output, &map_output).unwrap();
            });
        }

        #[bench]
        fn bench_es6_simple(b: &mut test::Bencher) {
            let entry_point = Path::new("examples/es6-simple/index.mjs");
            npm_install(entry_point.parent().unwrap());
            let options = InputOptions::default();
            let output = "/dev/null";
            let map_output = SourceMapOutput::Inline;

            b.iter(|| {
                let _ = bundle(&entry_point, options, &output, &map_output).unwrap();
            });
        }

        #[bench]
        fn bench_es6_everywhere_simple(b: &mut test::Bencher) {
            let entry_point = Path::new("examples/es6-everywhere-simple/index.js");
            npm_install(entry_point.parent().unwrap());
            let options = InputOptions::default();
            let output = "/dev/null";
            let map_output = SourceMapOutput::Inline;

            b.iter(|| {
                let _ = bundle(&entry_point, options, &output, &map_output).unwrap();
            });
        }

        #[bench]
        fn bench_write_map_to(b: &mut test::Bencher) {
            let writer = Writer {
                modules: {
                    let mut modules = FnvHashMap::default();
                    for i in 0..1000 {
                        let mut path = PathBuf::new();
                        path.push(i.to_string());
                        path.push("examples/es6-everywhere-simple/node_modules/itt/index.js");
                        modules.insert(
                            path,
                            Module {
                                source: Source {
                                    prefix: "~function() {".to_owned(),
                                    body: include_str!("itt.js").to_owned(),
                                    suffix: "}()".to_owned(),
                                    original: None,
                                },
                                deps: {
                                    let mut deps = FnvHashMap::new();
                                    deps.insert("./math".to_owned(), Resolved::Normal(
                                        Path::new("examples/es6-everywhere-simple/math.js").to_owned(),
                                    ));
                                    deps.insert("itt".to_owned(), Resolved::Normal(
                                        Path::new("examples/es6-everywhere-simple/node_modules/itt/index.js").to_owned(),
                                    ));
                                    deps
                                },
                            },
                        );
                    }
                    modules
                },
                entry_point: Path::new("examples/es6-everywhere-simple/index.js"),
                map_output: &SourceMapOutput::Inline,
            };

            let mut out = Vec::new();
            b.iter(|| {
                out.clear();
                writer.write_map_to(&mut out).unwrap();
            });
            b.bytes = out.len() as u64;
        }
    }
}
