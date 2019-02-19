#![allow(unused_imports)]

#[cfg(feature = "bench")]
extern crate test;

use serde_json;
use std::path::Path;
use std::process;
use super::*;

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
fn test_vlq() {
    // 0000000000000000111111111111111122222222222222223333333333333333
    // 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
    // ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/
    let mut vlq = Vlq::new();
    assert_eq!(vlq.enc(0), "A");
    assert_eq!(vlq.enc(1), "C");
    assert_eq!(vlq.enc(-1), "D");
    assert_eq!(vlq.enc(5), "K");
    assert_eq!(vlq.enc(-5), "L");
    assert_eq!(vlq.enc(15), "e");
    assert_eq!(vlq.enc(-15), "f");
    assert_eq!(vlq.enc(16), "gB");
    assert_eq!(vlq.enc(1876), "o1D"); // 11 10101 0100
    assert_eq!(vlq.enc(-485223), "v2zd"); // 11101 10011 10110 0111
}

#[test]
fn test_deserialize_browser_subst() {
    let parse = serde_json::from_str::<BrowserSubstitution<String>>;
    assert_matches!(parse("null"), Err(_));
    assert_matches!(parse("100"), Err(_));
    assert_matches!(parse("[1, 2, 3]"), Err(_));
    assert_matches!(parse("false"), Ok(BrowserSubstitution::Ignore));
    assert_matches!(parse("true"), Err(_));
    assert_eq!(parse(r#""asdf""#).unwrap(), BrowserSubstitution::Replace("asdf".to_owned()));
    assert_eq!(parse(r#""""#).unwrap(), BrowserSubstitution::Replace("".to_owned()));
}

#[test]
fn test_deserialize_browser() {
    let parse = serde_json::from_str::<BrowserSubstitutionMap>;
    assert_matches!(parse(r#"null"#), Err(_));
    assert_matches!(parse(r#""simple.browser.js""#), Err(_));
    assert_eq!(parse(r#"{}"#).unwrap(), BrowserSubstitutionMap(map!{}));
    assert_eq!(parse(r#"{"mod": "dom"}"#).unwrap(), BrowserSubstitutionMap(map!{
        PathBuf::from("mod") => BrowserSubstitution::Replace(PathBuf::from("dom")),
    }));
    assert_eq!(parse(r#"{"./file.js": "./file.browser.js"}"#).unwrap(), BrowserSubstitutionMap(map!{
        PathBuf::from("./file.js") => BrowserSubstitution::Replace(PathBuf::from("./file.browser.js")),
    }));
    assert_eq!(parse(r#"{"ignore": false}"#).unwrap(), BrowserSubstitutionMap(map!{
        PathBuf::from("ignore") => BrowserSubstitution::Ignore,
    }));
    assert_eq!(parse(r#"{
        "ignore": false,
        "mod": "dom",
        "mod2file": "./modfile.js",
        "mod2up": "../up.js",
        "mod2dir": "./moddir",
        "mod2abs": "/z/y/x",
        "./fileignore.js": false,
        "./file2mod.js": "mod",
        "./file2file.js": "./file.js",
        "./file2dir.js": "./dir",
        "./file2up.js": "../up.js",
        "./file2abs.js": "/x/y/z"
    }"#).unwrap(), BrowserSubstitutionMap(map!{
        PathBuf::from("ignore") => BrowserSubstitution::Ignore,
        PathBuf::from("mod") => BrowserSubstitution::Replace(PathBuf::from("dom")),
        PathBuf::from("mod2file") => BrowserSubstitution::Replace(PathBuf::from("./modfile.js")),
        PathBuf::from("mod2up") => BrowserSubstitution::Replace(PathBuf::from("../up.js")),
        PathBuf::from("mod2dir") => BrowserSubstitution::Replace(PathBuf::from("./moddir")),
        PathBuf::from("mod2abs") => BrowserSubstitution::Replace(PathBuf::from("/z/y/x")),
        PathBuf::from("./fileignore.js") => BrowserSubstitution::Ignore,
        PathBuf::from("./file2mod.js") => BrowserSubstitution::Replace(PathBuf::from("mod")),
        PathBuf::from("./file2file.js") => BrowserSubstitution::Replace(PathBuf::from("./file.js")),
        PathBuf::from("./file2dir.js") => BrowserSubstitution::Replace(PathBuf::from("./dir")),
        PathBuf::from("./file2up.js") => BrowserSubstitution::Replace(PathBuf::from("../up.js")),
        PathBuf::from("./file2abs.js") => BrowserSubstitution::Replace(PathBuf::from("/x/y/z")),
    }));
}

#[test]
fn test_deserialize_package_info() {
    let parse = serde_json::from_str::<PackageInfo>;
    assert_matches!(parse("null"), Err(_));
    assert_matches!(parse("100"), Err(_));
    assert_matches!(parse("[1, 2, 3]"), Err(_));
    assert_eq!(parse(r#"{}"#).unwrap(), PackageInfo {
        main: PathBuf::from("./index"),
        browser_substitutions: BrowserSubstitutionMap(map!{}),
    });
    assert_eq!(parse(r#"{"browser": null}"#).unwrap(), PackageInfo {
        main: PathBuf::from("./index"),
        browser_substitutions: BrowserSubstitutionMap(map!{}),
    });
    assert_eq!(parse(r#"{"browser": "simple"}"#).unwrap(), PackageInfo {
        main: PathBuf::from("./index"),
        browser_substitutions: BrowserSubstitutionMap(map!{
            PathBuf::from("./index") => BrowserSubstitution::Replace(PathBuf::from("./simple")),
        }),
    });
    assert_eq!(parse(r#"{"browser": {}}"#).unwrap(), PackageInfo {
        main: PathBuf::from("./index"),
        browser_substitutions: BrowserSubstitutionMap(map!{}),
    });
    assert_eq!(parse(r#"{"browser": {"mod": false}}"#).unwrap(), PackageInfo {
        main: PathBuf::from("./index"),
        browser_substitutions: BrowserSubstitutionMap(map!{
            PathBuf::from("mod") => BrowserSubstitution::Ignore,
        }),
    });
}

#[test]
fn test_resolve_path_or_module() {
    fn assert_resolves(from: &str, to: Option<&str>, input_options: &InputOptions) {
        // let mut base_path = PathBuf::from(file!());
        // base_path.append_resolving("../../../fixtures");
        let mut base_path = std::env::current_dir().unwrap();
        base_path.push("fixtures");
        let to_path = to.map(|to| {
            let mut to_path = base_path.clone();
            to_path.append_resolving(to);
            to_path
        });
        let mut from_path = base_path;
        from_path.append_resolving(from);

        let resolver = Resolver::new(input_options.clone());
        let expected = to_path.map(Resolved::Normal);
        // resolves with an empty cache...
        assert_eq!(resolver.resolve_path_or_module(None, from_path.clone()).unwrap(), expected);
        // ...and with everything cached
        assert_eq!(resolver.resolve_path_or_module(None, from_path).unwrap(), expected);
    }
    let cjs = InputOptions {
        for_browser: false,
        es6_syntax: false,
        es6_syntax_everywhere: false,
        external: Default::default(),
    };
    let esm = InputOptions {
        for_browser: false,
        es6_syntax: true,
        es6_syntax_everywhere: false,
        external: Default::default(),
    };
    assert_resolves("resolve/named-noext",
               Some("resolve/named-noext"), &cjs);
    assert_resolves("resolve/named-js.js",
               Some("resolve/named-js.js"), &cjs);
    assert_resolves("resolve/named-json.json",
               Some("resolve/named-json.json"), &cjs);
    assert_resolves("resolve/named-mjs.mjs",
               Some("resolve/named-mjs.mjs"), &esm);
    assert_resolves("resolve/named-jsz.jsz",
               Some("resolve/named-jsz.jsz"), &cjs);

    assert_resolves("resolve/named-js",
               Some("resolve/named-js.js"), &cjs);
    assert_resolves("resolve/named-json",
               Some("resolve/named-json.json"), &cjs);
    assert_resolves("resolve/named-mjs",
               Some("resolve/named-mjs.mjs"), &esm);

    assert_resolves("resolve/dir-js",
               Some("resolve/dir-js/index.js"), &cjs);
    assert_resolves("resolve/dir-js/index",
               Some("resolve/dir-js/index.js"), &cjs);
    assert_resolves("resolve/dir-json",
               Some("resolve/dir-json/index.json"), &cjs);
    assert_resolves("resolve/dir-json/index",
               Some("resolve/dir-json/index.json"), &cjs);
    assert_resolves("resolve/dir-mjs",
               Some("resolve/dir-mjs/index.mjs"), &esm);
    assert_resolves("resolve/dir-mjs/index",
               Some("resolve/dir-mjs/index.mjs"), &esm);

    assert_resolves("resolve/mod-noext-bare",
               Some("resolve/mod-noext-bare/main-noext"), &cjs);
    assert_resolves("resolve/mod-noext-rel",
               Some("resolve/mod-noext-rel/main-noext"), &cjs);

    assert_resolves("resolve/mod-main-nesting-bare",
               Some("resolve/mod-main-nesting-bare/subdir/index.js"), &cjs);
    assert_resolves("resolve/mod-main-nesting-bare/subdir",
               Some("resolve/mod-main-nesting-bare/subdir/inner-main.js"), &cjs);
    assert_resolves("resolve/mod-main-nesting-rel",
               Some("resolve/mod-main-nesting-rel/subdir/index.js"), &cjs);
    assert_resolves("resolve/mod-main-nesting-rel/subdir",
               Some("resolve/mod-main-nesting-rel/subdir/inner-main.js"), &cjs);

    assert_resolves("resolve/mod-js-ext-bare",
               Some("resolve/mod-js-ext-bare/main-js.js"), &cjs);
    assert_resolves("resolve/mod-js-ext-rel",
               Some("resolve/mod-js-ext-rel/main-js.js"), &cjs);
    assert_resolves("resolve/mod-js-noext-bare",
               Some("resolve/mod-js-noext-bare/main-js.js"), &cjs);
    assert_resolves("resolve/mod-js-noext-rel",
               Some("resolve/mod-js-noext-rel/main-js.js"), &cjs);
    assert_resolves("resolve/mod-js-dir-bare",
               Some("resolve/mod-js-dir-bare/main-js/index.js"), &cjs);
    assert_resolves("resolve/mod-js-dir-rel",
               Some("resolve/mod-js-dir-rel/main-js/index.js"), &cjs);

    assert_resolves("resolve/mod-json-ext-bare",
               Some("resolve/mod-json-ext-bare/main-json.json"), &cjs);
    assert_resolves("resolve/mod-json-ext-rel",
               Some("resolve/mod-json-ext-rel/main-json.json"), &cjs);
    assert_resolves("resolve/mod-json-noext-bare",
               Some("resolve/mod-json-noext-bare/main-json.json"), &cjs);
    assert_resolves("resolve/mod-json-noext-rel",
               Some("resolve/mod-json-noext-rel/main-json.json"), &cjs);
    assert_resolves("resolve/mod-json-dir-bare",
               Some("resolve/mod-json-dir-bare/main-json/index.json"), &cjs);
    assert_resolves("resolve/mod-json-dir-rel",
               Some("resolve/mod-json-dir-rel/main-json/index.json"), &cjs);

    assert_resolves("resolve/mod-mjs-ext-bare/main-mjs",
               Some("resolve/mod-mjs-ext-bare/main-mjs.mjs"), &esm);
    assert_resolves("resolve/mod-mjs-ext-rel/main-mjs",
               Some("resolve/mod-mjs-ext-rel/main-mjs.mjs"), &esm);
    assert_resolves("resolve/mod-mjs-noext-bare/main-mjs",
               Some("resolve/mod-mjs-noext-bare/main-mjs.mjs"), &esm);
    assert_resolves("resolve/mod-mjs-noext-rel/main-mjs",
               Some("resolve/mod-mjs-noext-rel/main-mjs.mjs"), &esm);
    assert_resolves("resolve/mod-mjs-dir-bare/main-mjs",
               Some("resolve/mod-mjs-dir-bare/main-mjs/index.mjs"), &esm);
    assert_resolves("resolve/mod-mjs-dir-rel/main-mjs",
               Some("resolve/mod-mjs-dir-rel/main-mjs/index.mjs"), &esm);

    assert_resolves("resolve/named-jsz", None, &cjs);
}

cfg_if! {
    if #[cfg(feature = "bench")] {
        fn npm_install(dir: &Path) {
            let node_modules = dir.join("node_modules");
            if node_modules.is_dir() { return }

            let ok = process::Command::new("npm")
                .arg("install")
                .arg("--silent")
                .current_dir(dir)
                .spawn()
                .expect("failed to start `npm install`")
                .wait()
                .unwrap()
                .success();
            if !ok {
                panic!("`npm install` did not exit successfully");
            }
        }

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
            let input_options = InputOptions::default();
            let output = "/dev/null";
            let map_output = SourceMapOutput::Inline;

            b.iter(|| {
                let _ = bundle(&entry_point, input_options, &output, &map_output).unwrap();
            });
        }

        #[bench]
        fn bench_es6_simple(b: &mut test::Bencher) {
            let entry_point = Path::new("examples/es6-simple/index.mjs");
            npm_install(entry_point.parent().unwrap());
            let input_options = InputOptions {
                es6_syntax: true,
                ..InputOptions::default()
            };
            let output = "/dev/null";
            let map_output = SourceMapOutput::Inline;

            b.iter(|| {
                let _ = bundle(&entry_point, input_options, &output, &map_output).unwrap();
            });
        }

        #[bench]
        fn bench_es6_everywhere_simple(b: &mut test::Bencher) {
            let entry_point = Path::new("examples/es6-everywhere-simple/index.js");
            npm_install(entry_point.parent().unwrap());
            let input_options = InputOptions {
                es6_syntax: true,
                es6_syntax_everywhere: true,
                ..InputOptions::default()
            };
            let output = "/dev/null";
            let map_output = SourceMapOutput::Inline;

            b.iter(|| {
                let _ = bundle(&entry_point, input_options, &output, &map_output).unwrap();
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
