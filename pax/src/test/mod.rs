#![allow(unused_imports)]

#[cfg(feature = "bench")]
extern crate test;

use serde_json;
use std::io::{self, Write};
use std::fs;
use std::path::Path;
use std::process;
use walkdir::WalkDir;
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

fn fixture_path() -> PathBuf {
    // let mut path = PathBuf::from(file!());
    // path.append_resolving("../../../fixtures");
    let mut path = std::env::current_dir().unwrap();
    path.push("fixtures");
    path
}

#[test]
fn test_resolve_path_or_module() {
    fn path_resolves(from: &str, to: Option<&str>, input_options: &InputOptions) {
        let base_path = fixture_path();
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
    path_resolves("resolve/named-noext",
             Some("resolve/named-noext"), &cjs);
    path_resolves("resolve/named-js.js",
             Some("resolve/named-js.js"), &cjs);
    path_resolves("resolve/named-json.json",
             Some("resolve/named-json.json"), &cjs);
    path_resolves("resolve/named-mjs.mjs",
             Some("resolve/named-mjs.mjs"), &esm);
    path_resolves("resolve/named-jsz.jsz",
             Some("resolve/named-jsz.jsz"), &cjs);

    path_resolves("resolve/named-js",
             Some("resolve/named-js.js"), &cjs);
    path_resolves("resolve/named-json",
             Some("resolve/named-json.json"), &cjs);
    path_resolves("resolve/named-mjs",
             Some("resolve/named-mjs.mjs"), &esm);

    path_resolves("resolve/dir-js",
             Some("resolve/dir-js/index.js"), &cjs);
    path_resolves("resolve/dir-js/index",
             Some("resolve/dir-js/index.js"), &cjs);
    path_resolves("resolve/dir-json",
             Some("resolve/dir-json/index.json"), &cjs);
    path_resolves("resolve/dir-json/index",
             Some("resolve/dir-json/index.json"), &cjs);
    path_resolves("resolve/dir-mjs",
             Some("resolve/dir-mjs/index.mjs"), &esm);
    path_resolves("resolve/dir-mjs/index",
             Some("resolve/dir-mjs/index.mjs"), &esm);

    path_resolves("resolve/mod-noext-bare",
             Some("resolve/mod-noext-bare/main-noext"), &cjs);
    path_resolves("resolve/mod-noext-rel",
             Some("resolve/mod-noext-rel/main-noext"), &cjs);

    path_resolves("resolve/mod-main-nesting-bare",
             Some("resolve/mod-main-nesting-bare/subdir/index.js"), &cjs);
    path_resolves("resolve/mod-main-nesting-bare/subdir",
             Some("resolve/mod-main-nesting-bare/subdir/inner-main.js"), &cjs);
    path_resolves("resolve/mod-main-nesting-rel",
             Some("resolve/mod-main-nesting-rel/subdir/index.js"), &cjs);
    path_resolves("resolve/mod-main-nesting-rel/subdir",
             Some("resolve/mod-main-nesting-rel/subdir/inner-main.js"), &cjs);

    path_resolves("resolve/mod-js-ext-bare",
             Some("resolve/mod-js-ext-bare/main-js.js"), &cjs);
    path_resolves("resolve/mod-js-ext-rel",
             Some("resolve/mod-js-ext-rel/main-js.js"), &cjs);
    path_resolves("resolve/mod-js-noext-bare",
             Some("resolve/mod-js-noext-bare/main-js.js"), &cjs);
    path_resolves("resolve/mod-js-noext-rel",
             Some("resolve/mod-js-noext-rel/main-js.js"), &cjs);
    path_resolves("resolve/mod-js-dir-bare",
             Some("resolve/mod-js-dir-bare/main-js/index.js"), &cjs);
    path_resolves("resolve/mod-js-dir-rel",
             Some("resolve/mod-js-dir-rel/main-js/index.js"), &cjs);

    path_resolves("resolve/mod-json-ext-bare",
             Some("resolve/mod-json-ext-bare/main-json.json"), &cjs);
    path_resolves("resolve/mod-json-ext-rel",
             Some("resolve/mod-json-ext-rel/main-json.json"), &cjs);
    path_resolves("resolve/mod-json-noext-bare",
             Some("resolve/mod-json-noext-bare/main-json.json"), &cjs);
    path_resolves("resolve/mod-json-noext-rel",
             Some("resolve/mod-json-noext-rel/main-json.json"), &cjs);
    path_resolves("resolve/mod-json-dir-bare",
             Some("resolve/mod-json-dir-bare/main-json/index.json"), &cjs);
    path_resolves("resolve/mod-json-dir-rel",
             Some("resolve/mod-json-dir-rel/main-json/index.json"), &cjs);

    path_resolves("resolve/mod-mjs-ext-bare",
             Some("resolve/mod-mjs-ext-bare/main-mjs.mjs"), &esm);
    path_resolves("resolve/mod-mjs-ext-rel",
             Some("resolve/mod-mjs-ext-rel/main-mjs.mjs"), &esm);
    path_resolves("resolve/mod-mjs-noext-bare",
             Some("resolve/mod-mjs-noext-bare/main-mjs.mjs"), &esm);
    path_resolves("resolve/mod-mjs-noext-rel",
             Some("resolve/mod-mjs-noext-rel/main-mjs.mjs"), &esm);
    path_resolves("resolve/mod-mjs-dir-bare",
             Some("resolve/mod-mjs-dir-bare/main-mjs/index.mjs"), &esm);
    path_resolves("resolve/mod-mjs-dir-rel",
             Some("resolve/mod-mjs-dir-rel/main-mjs/index.mjs"), &esm);

    path_resolves("resolve/named-jsz", None, &cjs);
}

fn assert_resolves(context: &str, from: &str, to: Option<&str>, input_options: &InputOptions) {
    let base_path = fixture_path();
    let to_path = to.map(|to| {
        let mut to_path = base_path.clone();
        to_path.append_resolving(to);
        to_path
    });
    let mut context_path = base_path;
    context_path.append_resolving(context);

    let resolver = Resolver::new(input_options.clone());
    if let Some(expected) = to_path.map(Resolved::Normal) {
        // resolves with an empty cache...
        assert_eq!(resolver.resolve(&context_path, from).unwrap(), expected);
        // ...and with everything cached
        assert_eq!(resolver.resolve(&context_path, from).unwrap(), expected);
    } else {
        // resolves with an empty cache...
        assert_matches!(resolver.resolve(&context_path, from), Err(_));
        // ...and with everything cached
        assert_matches!(resolver.resolve(&context_path, from), Err(_));
    }
}

#[test]
fn test_resolve() {
  test_resolve_with(assert_resolves);
}
fn test_resolve_with<F>(mut assert_resolves: F)
where F: FnMut(&str, &str, Option<&str>, &InputOptions) {
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

    // relative paths

    let ctx = "resolve/hypothetical.js";
    assert_resolves(ctx, "./named-noext",
              Some("resolve/named-noext"), &cjs);
    assert_resolves(ctx, "./named-js.js",
              Some("resolve/named-js.js"), &cjs);
    assert_resolves(ctx, "./named-json.json",
              Some("resolve/named-json.json"), &cjs);
    assert_resolves(ctx, "./named-mjs.mjs",
              Some("resolve/named-mjs.mjs"), &esm);
    assert_resolves(ctx, "./named-jsz.jsz",
              Some("resolve/named-jsz.jsz"), &cjs);

    assert_resolves(ctx, "./named-js",
              Some("resolve/named-js.js"), &cjs);
    assert_resolves(ctx, "./named-json",
              Some("resolve/named-json.json"), &cjs);
    assert_resolves(ctx, "./named-mjs",
              Some("resolve/named-mjs.mjs"), &esm);

    assert_resolves(ctx, "./dir-js",
              Some("resolve/dir-js/index.js"), &cjs);
    assert_resolves(ctx, "./dir-js/index",
              Some("resolve/dir-js/index.js"), &cjs);
    assert_resolves(ctx, "./dir-json",
              Some("resolve/dir-json/index.json"), &cjs);
    assert_resolves(ctx, "./dir-json/index",
              Some("resolve/dir-json/index.json"), &cjs);
    assert_resolves(ctx, "./dir-mjs",
              Some("resolve/dir-mjs/index.mjs"), &esm);
    assert_resolves(ctx, "./dir-mjs/index",
              Some("resolve/dir-mjs/index.mjs"), &esm);

    assert_resolves(ctx, "./mod-noext-bare",
              Some("resolve/mod-noext-bare/main-noext"), &cjs);
    assert_resolves(ctx, "./mod-noext-rel",
              Some("resolve/mod-noext-rel/main-noext"), &cjs);

    assert_resolves(ctx, "./mod-main-nesting-bare",
              Some("resolve/mod-main-nesting-bare/subdir/index.js"), &cjs);
    assert_resolves(ctx, "./mod-main-nesting-bare/subdir",
              Some("resolve/mod-main-nesting-bare/subdir/inner-main.js"), &cjs);
    assert_resolves(ctx, "./mod-main-nesting-rel",
              Some("resolve/mod-main-nesting-rel/subdir/index.js"), &cjs);
    assert_resolves(ctx, "./mod-main-nesting-rel/subdir",
              Some("resolve/mod-main-nesting-rel/subdir/inner-main.js"), &cjs);

    assert_resolves(ctx, "./mod-js-ext-bare",
              Some("resolve/mod-js-ext-bare/main-js.js"), &cjs);
    assert_resolves(ctx, "./mod-js-ext-rel",
              Some("resolve/mod-js-ext-rel/main-js.js"), &cjs);
    assert_resolves(ctx, "./mod-js-noext-bare",
              Some("resolve/mod-js-noext-bare/main-js.js"), &cjs);
    assert_resolves(ctx, "./mod-js-noext-rel",
              Some("resolve/mod-js-noext-rel/main-js.js"), &cjs);
    assert_resolves(ctx, "./mod-js-dir-bare",
              Some("resolve/mod-js-dir-bare/main-js/index.js"), &cjs);
    assert_resolves(ctx, "./mod-js-dir-rel",
              Some("resolve/mod-js-dir-rel/main-js/index.js"), &cjs);

    assert_resolves(ctx, "./mod-json-ext-bare",
              Some("resolve/mod-json-ext-bare/main-json.json"), &cjs);
    assert_resolves(ctx, "./mod-json-ext-rel",
              Some("resolve/mod-json-ext-rel/main-json.json"), &cjs);
    assert_resolves(ctx, "./mod-json-noext-bare",
              Some("resolve/mod-json-noext-bare/main-json.json"), &cjs);
    assert_resolves(ctx, "./mod-json-noext-rel",
              Some("resolve/mod-json-noext-rel/main-json.json"), &cjs);
    assert_resolves(ctx, "./mod-json-dir-bare",
              Some("resolve/mod-json-dir-bare/main-json/index.json"), &cjs);
    assert_resolves(ctx, "./mod-json-dir-rel",
              Some("resolve/mod-json-dir-rel/main-json/index.json"), &cjs);

    assert_resolves(ctx, "./mod-mjs-ext-bare",
              Some("resolve/mod-mjs-ext-bare/main-mjs.mjs"), &esm);
    assert_resolves(ctx, "./mod-mjs-ext-rel",
              Some("resolve/mod-mjs-ext-rel/main-mjs.mjs"), &esm);
    assert_resolves(ctx, "./mod-mjs-noext-bare",
              Some("resolve/mod-mjs-noext-bare/main-mjs.mjs"), &esm);
    assert_resolves(ctx, "./mod-mjs-noext-rel",
              Some("resolve/mod-mjs-noext-rel/main-mjs.mjs"), &esm);
    assert_resolves(ctx, "./mod-mjs-dir-bare",
              Some("resolve/mod-mjs-dir-bare/main-mjs/index.mjs"), &esm);
    assert_resolves(ctx, "./mod-mjs-dir-rel",
              Some("resolve/mod-mjs-dir-rel/main-mjs/index.mjs"), &esm);

    assert_resolves(ctx, "./mod-js-slash-bare",
              Some("resolve/mod-js-slash-bare/main.js"), &cjs);
    assert_resolves(ctx, "./mod-js-slash-rel",
              Some("resolve/mod-js-slash-rel/main.js"), &cjs);

    assert_resolves(ctx, "./named-jsz", None, &cjs);

    assert_resolves(ctx, "./file-and-dir",
              Some("resolve/file-and-dir.js"), &cjs);
    assert_resolves(ctx, "./file-and-dir/",
              Some("resolve/file-and-dir/index.js"), &cjs);
    assert_resolves(ctx, "./file-and-mod",
              Some("resolve/file-and-mod.js"), &cjs);
    assert_resolves(ctx, "./file-and-mod/",
              Some("resolve/file-and-mod/main.js"), &cjs);
    assert_resolves(ctx, "./dir-js/",
              Some("resolve/dir-js/index.js"), &cjs);
    assert_resolves(ctx, "./mod-js-noext-rel/",
              Some("resolve/mod-js-noext-rel/main-js.js"), &cjs);
    assert_resolves(ctx, "./named-js.js/", None, &cjs);
    assert_resolves(ctx, "./named-js/", None, &cjs);
    assert_resolves(ctx, "./named-noext/", None, &cjs);

    let ctx = "resolve/subdir/hypothetical.js";
    assert_resolves(ctx, "./named-js", None, &cjs);

    assert_resolves(ctx, "../named-noext",
               Some("resolve/named-noext"), &cjs);
    assert_resolves(ctx, "../named-js.js",
               Some("resolve/named-js.js"), &cjs);
    assert_resolves(ctx, "../named-json.json",
               Some("resolve/named-json.json"), &cjs);
    assert_resolves(ctx, "../named-mjs.mjs",
               Some("resolve/named-mjs.mjs"), &esm);
    assert_resolves(ctx, "../named-jsz.jsz",
               Some("resolve/named-jsz.jsz"), &cjs);

    assert_resolves(ctx, "../named-js",
               Some("resolve/named-js.js"), &cjs);
    assert_resolves(ctx, "../named-json",
               Some("resolve/named-json.json"), &cjs);
    assert_resolves(ctx, "../named-mjs",
               Some("resolve/named-mjs.mjs"), &esm);

    assert_resolves(ctx, "../dir-js",
               Some("resolve/dir-js/index.js"), &cjs);
    assert_resolves(ctx, "../dir-js/index",
               Some("resolve/dir-js/index.js"), &cjs);
    assert_resolves(ctx, "../dir-json",
               Some("resolve/dir-json/index.json"), &cjs);
    assert_resolves(ctx, "../dir-json/index",
               Some("resolve/dir-json/index.json"), &cjs);
    assert_resolves(ctx, "../dir-mjs",
               Some("resolve/dir-mjs/index.mjs"), &esm);
    assert_resolves(ctx, "../dir-mjs/index",
               Some("resolve/dir-mjs/index.mjs"), &esm);

    assert_resolves(ctx, "../mod-noext-bare",
               Some("resolve/mod-noext-bare/main-noext"), &cjs);
    assert_resolves(ctx, "../mod-noext-rel",
               Some("resolve/mod-noext-rel/main-noext"), &cjs);

    assert_resolves(ctx, "../mod-main-nesting-bare",
               Some("resolve/mod-main-nesting-bare/subdir/index.js"), &cjs);
    assert_resolves(ctx, "../mod-main-nesting-bare/subdir",
               Some("resolve/mod-main-nesting-bare/subdir/inner-main.js"), &cjs);
    assert_resolves(ctx, "../mod-main-nesting-rel",
               Some("resolve/mod-main-nesting-rel/subdir/index.js"), &cjs);
    assert_resolves(ctx, "../mod-main-nesting-rel/subdir",
               Some("resolve/mod-main-nesting-rel/subdir/inner-main.js"), &cjs);

    assert_resolves(ctx, "../mod-js-ext-bare",
               Some("resolve/mod-js-ext-bare/main-js.js"), &cjs);
    assert_resolves(ctx, "../mod-js-ext-rel",
               Some("resolve/mod-js-ext-rel/main-js.js"), &cjs);
    assert_resolves(ctx, "../mod-js-noext-bare",
               Some("resolve/mod-js-noext-bare/main-js.js"), &cjs);
    assert_resolves(ctx, "../mod-js-noext-rel",
               Some("resolve/mod-js-noext-rel/main-js.js"), &cjs);
    assert_resolves(ctx, "../mod-js-dir-bare",
               Some("resolve/mod-js-dir-bare/main-js/index.js"), &cjs);
    assert_resolves(ctx, "../mod-js-dir-rel",
               Some("resolve/mod-js-dir-rel/main-js/index.js"), &cjs);

    assert_resolves(ctx, "../mod-json-ext-bare",
               Some("resolve/mod-json-ext-bare/main-json.json"), &cjs);
    assert_resolves(ctx, "../mod-json-ext-rel",
               Some("resolve/mod-json-ext-rel/main-json.json"), &cjs);
    assert_resolves(ctx, "../mod-json-noext-bare",
               Some("resolve/mod-json-noext-bare/main-json.json"), &cjs);
    assert_resolves(ctx, "../mod-json-noext-rel",
               Some("resolve/mod-json-noext-rel/main-json.json"), &cjs);
    assert_resolves(ctx, "../mod-json-dir-bare",
               Some("resolve/mod-json-dir-bare/main-json/index.json"), &cjs);
    assert_resolves(ctx, "../mod-json-dir-rel",
               Some("resolve/mod-json-dir-rel/main-json/index.json"), &cjs);

    assert_resolves(ctx, "../mod-mjs-ext-bare",
               Some("resolve/mod-mjs-ext-bare/main-mjs.mjs"), &esm);
    assert_resolves(ctx, "../mod-mjs-ext-rel",
               Some("resolve/mod-mjs-ext-rel/main-mjs.mjs"), &esm);
    assert_resolves(ctx, "../mod-mjs-noext-bare",
               Some("resolve/mod-mjs-noext-bare/main-mjs.mjs"), &esm);
    assert_resolves(ctx, "../mod-mjs-noext-rel",
               Some("resolve/mod-mjs-noext-rel/main-mjs.mjs"), &esm);
    assert_resolves(ctx, "../mod-mjs-dir-bare",
               Some("resolve/mod-mjs-dir-bare/main-mjs/index.mjs"), &esm);
    assert_resolves(ctx, "../mod-mjs-dir-rel",
               Some("resolve/mod-mjs-dir-rel/main-mjs/index.mjs"), &esm);

    assert_resolves(ctx, "../mod-js-slash-bare",
               Some("resolve/mod-js-slash-bare/main.js"), &cjs);
    assert_resolves(ctx, "../mod-js-slash-rel",
               Some("resolve/mod-js-slash-rel/main.js"), &cjs);

    assert_resolves(ctx, "../named-jsz", None, &cjs);

    assert_resolves(ctx, "../file-and-dir",
               Some("resolve/file-and-dir.js"), &cjs);
    assert_resolves(ctx, "../file-and-dir/",
               Some("resolve/file-and-dir/index.js"), &cjs);
    assert_resolves(ctx, "../file-and-mod",
               Some("resolve/file-and-mod.js"), &cjs);
    assert_resolves(ctx, "../file-and-mod/",
               Some("resolve/file-and-mod/main.js"), &cjs);
    assert_resolves(ctx, "../dir-js/",
               Some("resolve/dir-js/index.js"), &cjs);
    assert_resolves(ctx, "../mod-js-noext-rel/",
               Some("resolve/mod-js-noext-rel/main-js.js"), &cjs);
    assert_resolves(ctx, "../named-js.js/", None, &cjs);
    assert_resolves(ctx, "../named-js/", None, &cjs);
    assert_resolves(ctx, "../named-noext/", None, &cjs);

    assert_resolves(ctx, "../mod-self-slash",
               Some("resolve/mod-self-slash/index.js"), &esm);
    assert_resolves(ctx, "../mod-self-slash/",
               Some("resolve/mod-self-slash/index.js"), &esm);
    assert_resolves(ctx, "../mod-self-noslash",
               Some("resolve/mod-self-noslash/index.js"), &esm);
    assert_resolves(ctx, "../mod-self-noslash/",
               Some("resolve/mod-self-noslash/index.js"), &esm);
    assert_resolves(ctx, "../mod-outer/mod-parent-slash",
               Some("resolve/mod-outer/index.js"), &esm);
    assert_resolves(ctx, "../mod-outer/mod-parent-slash/",
               Some("resolve/mod-outer/index.js"), &esm);
    assert_resolves(ctx, "../mod-outer/mod-parent-noslash",
               Some("resolve/mod-outer/index.js"), &esm);
    assert_resolves(ctx, "../mod-outer/mod-parent-noslash/",
               Some("resolve/mod-outer/index.js"), &esm);

    assert_resolves("resolve/dir-js/hypothetical.js", ".",
               Some("resolve/dir-js/index.js"), &cjs);
    assert_resolves("resolve/dir-js/hypothetical.js", "./",
               Some("resolve/dir-js/index.js"), &cjs);
    assert_resolves("resolve/dir-json/hypothetical.js", ".",
               Some("resolve/dir-json/index.json"), &cjs);
    assert_resolves("resolve/dir-json/hypothetical.js", "./",
               Some("resolve/dir-json/index.json"), &cjs);
    assert_resolves("resolve/dir-mjs/hypothetical.js", ".",
               Some("resolve/dir-mjs/index.mjs"), &esm);
    assert_resolves("resolve/dir-mjs/hypothetical.js", "./",
               Some("resolve/dir-mjs/index.mjs"), &esm);

    assert_resolves("resolve/mod-noext-bare/hypothetical.js", ".",
               Some("resolve/mod-noext-bare/main-noext"), &cjs);
    assert_resolves("resolve/mod-noext-bare/hypothetical.js", "./",
               Some("resolve/mod-noext-bare/main-noext"), &cjs);
    assert_resolves("resolve/mod-noext-rel/hypothetical.js", ".",
               Some("resolve/mod-noext-rel/main-noext"), &cjs);
    assert_resolves("resolve/mod-noext-rel/hypothetical.js", "./",
               Some("resolve/mod-noext-rel/main-noext"), &cjs);

    assert_resolves("resolve/mod-main-nesting-bare/hypothetical.js", ".",
               Some("resolve/mod-main-nesting-bare/subdir/index.js"), &cjs);
    assert_resolves("resolve/mod-main-nesting-bare/hypothetical.js", "./",
               Some("resolve/mod-main-nesting-bare/subdir/index.js"), &cjs);
    assert_resolves("resolve/mod-main-nesting-bare/subdir/hypothetical.js", ".",
               Some("resolve/mod-main-nesting-bare/subdir/inner-main.js"), &cjs);
    assert_resolves("resolve/mod-main-nesting-bare/subdir/hypothetical.js", "./",
               Some("resolve/mod-main-nesting-bare/subdir/inner-main.js"), &cjs);
    assert_resolves("resolve/mod-main-nesting-rel/hypothetical.js", ".",
               Some("resolve/mod-main-nesting-rel/subdir/index.js"), &cjs);
    assert_resolves("resolve/mod-main-nesting-rel/hypothetical.js", "./",
               Some("resolve/mod-main-nesting-rel/subdir/index.js"), &cjs);
    assert_resolves("resolve/mod-main-nesting-rel/subdir/hypothetical.js", "..",
               Some("resolve/mod-main-nesting-rel/subdir/index.js"), &cjs);
    assert_resolves("resolve/mod-main-nesting-rel/subdir/hypothetical.js", "../",
               Some("resolve/mod-main-nesting-rel/subdir/index.js"), &cjs);
    assert_resolves("resolve/mod-main-nesting-rel/subdir/hypothetical.js", ".",
               Some("resolve/mod-main-nesting-rel/subdir/inner-main.js"), &cjs);
    assert_resolves("resolve/mod-main-nesting-rel/subdir/hypothetical.js", "./",
               Some("resolve/mod-main-nesting-rel/subdir/inner-main.js"), &cjs);

    assert_resolves("resolve/mod-js-ext-bare/hypothetical.js", ".",
               Some("resolve/mod-js-ext-bare/main-js.js"), &cjs);
    assert_resolves("resolve/mod-js-ext-bare/hypothetical.js", "./",
               Some("resolve/mod-js-ext-bare/main-js.js"), &cjs);
    assert_resolves("resolve/mod-js-ext-rel/hypothetical.js", ".",
               Some("resolve/mod-js-ext-rel/main-js.js"), &cjs);
    assert_resolves("resolve/mod-js-ext-rel/hypothetical.js", "./",
               Some("resolve/mod-js-ext-rel/main-js.js"), &cjs);
    assert_resolves("resolve/mod-js-noext-bare/hypothetical.js", ".",
               Some("resolve/mod-js-noext-bare/main-js.js"), &cjs);
    assert_resolves("resolve/mod-js-noext-bare/hypothetical.js", "./",
               Some("resolve/mod-js-noext-bare/main-js.js"), &cjs);
    assert_resolves("resolve/mod-js-noext-rel/hypothetical.js", ".",
               Some("resolve/mod-js-noext-rel/main-js.js"), &cjs);
    assert_resolves("resolve/mod-js-noext-rel/hypothetical.js", "./",
               Some("resolve/mod-js-noext-rel/main-js.js"), &cjs);
    assert_resolves("resolve/mod-js-dir-bare/hypothetical.js", ".",
               Some("resolve/mod-js-dir-bare/main-js/index.js"), &cjs);
    assert_resolves("resolve/mod-js-dir-bare/hypothetical.js", "./",
               Some("resolve/mod-js-dir-bare/main-js/index.js"), &cjs);
    assert_resolves("resolve/mod-js-dir-bare/main-js/hypothetical.js", "..",
               Some("resolve/mod-js-dir-bare/main-js/index.js"), &cjs);
    assert_resolves("resolve/mod-js-dir-bare/main-js/hypothetical.js", "../",
               Some("resolve/mod-js-dir-bare/main-js/index.js"), &cjs);
    assert_resolves("resolve/mod-js-dir-rel/hypothetical.js", ".",
               Some("resolve/mod-js-dir-rel/main-js/index.js"), &cjs);
    assert_resolves("resolve/mod-js-dir-rel/hypothetical.js", "./",
               Some("resolve/mod-js-dir-rel/main-js/index.js"), &cjs);
    assert_resolves("resolve/mod-js-dir-rel/main-js/hypothetical.js", "..",
               Some("resolve/mod-js-dir-rel/main-js/index.js"), &cjs);
    assert_resolves("resolve/mod-js-dir-rel/main-js/hypothetical.js", "../",
               Some("resolve/mod-js-dir-rel/main-js/index.js"), &cjs);

    assert_resolves("resolve/mod-json-ext-bare/hypothetical.js", ".",
               Some("resolve/mod-json-ext-bare/main-json.json"), &cjs);
    assert_resolves("resolve/mod-json-ext-bare/hypothetical.js", "./",
               Some("resolve/mod-json-ext-bare/main-json.json"), &cjs);
    assert_resolves("resolve/mod-json-ext-rel/hypothetical.js", ".",
               Some("resolve/mod-json-ext-rel/main-json.json"), &cjs);
    assert_resolves("resolve/mod-json-ext-rel/hypothetical.js", "./",
               Some("resolve/mod-json-ext-rel/main-json.json"), &cjs);
    assert_resolves("resolve/mod-json-noext-bare/hypothetical.js", ".",
               Some("resolve/mod-json-noext-bare/main-json.json"), &cjs);
    assert_resolves("resolve/mod-json-noext-bare/hypothetical.js", "./",
               Some("resolve/mod-json-noext-bare/main-json.json"), &cjs);
    assert_resolves("resolve/mod-json-noext-rel/hypothetical.js", ".",
               Some("resolve/mod-json-noext-rel/main-json.json"), &cjs);
    assert_resolves("resolve/mod-json-noext-rel/hypothetical.js", "./",
               Some("resolve/mod-json-noext-rel/main-json.json"), &cjs);
    assert_resolves("resolve/mod-json-dir-bare/hypothetical.js", ".",
               Some("resolve/mod-json-dir-bare/main-json/index.json"), &cjs);
    assert_resolves("resolve/mod-json-dir-bare/hypothetical.js", "./",
               Some("resolve/mod-json-dir-bare/main-json/index.json"), &cjs);
    assert_resolves("resolve/mod-json-dir-rel/hypothetical.js", ".",
               Some("resolve/mod-json-dir-rel/main-json/index.json"), &cjs);
    assert_resolves("resolve/mod-json-dir-rel/hypothetical.js", "./",
               Some("resolve/mod-json-dir-rel/main-json/index.json"), &cjs);

    assert_resolves("resolve/mod-mjs-ext-bare/hypothetical.js", ".",
               Some("resolve/mod-mjs-ext-bare/main-mjs.mjs"), &esm);
    assert_resolves("resolve/mod-mjs-ext-bare/hypothetical.js", "./",
               Some("resolve/mod-mjs-ext-bare/main-mjs.mjs"), &esm);
    assert_resolves("resolve/mod-mjs-ext-rel/hypothetical.js", ".",
               Some("resolve/mod-mjs-ext-rel/main-mjs.mjs"), &esm);
    assert_resolves("resolve/mod-mjs-ext-rel/hypothetical.js", "./",
               Some("resolve/mod-mjs-ext-rel/main-mjs.mjs"), &esm);
    assert_resolves("resolve/mod-mjs-noext-bare/hypothetical.js", ".",
               Some("resolve/mod-mjs-noext-bare/main-mjs.mjs"), &esm);
    assert_resolves("resolve/mod-mjs-noext-bare/hypothetical.js", "./",
               Some("resolve/mod-mjs-noext-bare/main-mjs.mjs"), &esm);
    assert_resolves("resolve/mod-mjs-noext-rel/hypothetical.js", ".",
               Some("resolve/mod-mjs-noext-rel/main-mjs.mjs"), &esm);
    assert_resolves("resolve/mod-mjs-noext-rel/hypothetical.js", "./",
               Some("resolve/mod-mjs-noext-rel/main-mjs.mjs"), &esm);
    assert_resolves("resolve/mod-mjs-dir-bare/hypothetical.js", ".",
               Some("resolve/mod-mjs-dir-bare/main-mjs/index.mjs"), &esm);
    assert_resolves("resolve/mod-mjs-dir-bare/hypothetical.js", "./",
               Some("resolve/mod-mjs-dir-bare/main-mjs/index.mjs"), &esm);
    assert_resolves("resolve/mod-mjs-dir-bare/main-mjs/hypothetical.js", "..",
               Some("resolve/mod-mjs-dir-bare/main-mjs/index.mjs"), &esm);
    assert_resolves("resolve/mod-mjs-dir-bare/main-mjs/hypothetical.js", "../",
               Some("resolve/mod-mjs-dir-bare/main-mjs/index.mjs"), &esm);
    assert_resolves("resolve/mod-mjs-dir-rel/hypothetical.js", ".",
               Some("resolve/mod-mjs-dir-rel/main-mjs/index.mjs"), &esm);
    assert_resolves("resolve/mod-mjs-dir-rel/hypothetical.js", "./",
               Some("resolve/mod-mjs-dir-rel/main-mjs/index.mjs"), &esm);
    assert_resolves("resolve/mod-mjs-dir-rel/main-mjs/hypothetical.js", "..",
               Some("resolve/mod-mjs-dir-rel/main-mjs/index.mjs"), &esm);
    assert_resolves("resolve/mod-mjs-dir-rel/main-mjs/hypothetical.js", "../",
               Some("resolve/mod-mjs-dir-rel/main-mjs/index.mjs"), &esm);

    assert_resolves("resolve/mod-js-slash-bare/hypothetical.js", ".",
               Some("resolve/mod-js-slash-bare/main.js"), &cjs);
    assert_resolves("resolve/mod-js-slash-bare/hypothetical.js", "./",
               Some("resolve/mod-js-slash-bare/main.js"), &cjs);
    assert_resolves("resolve/mod-js-slash-bare/main/hypothetical.js", "..",
               Some("resolve/mod-js-slash-bare/main.js"), &cjs);
    assert_resolves("resolve/mod-js-slash-bare/main/hypothetical.js", "../",
               Some("resolve/mod-js-slash-bare/main.js"), &cjs);
    assert_resolves("resolve/mod-js-slash-rel/hypothetical.js", ".",
               Some("resolve/mod-js-slash-rel/main.js"), &cjs);
    assert_resolves("resolve/mod-js-slash-rel/hypothetical.js", "./",
               Some("resolve/mod-js-slash-rel/main.js"), &cjs);
    assert_resolves("resolve/mod-js-slash-rel/main/hypothetical.js", "..",
               Some("resolve/mod-js-slash-rel/main.js"), &cjs);
    assert_resolves("resolve/mod-js-slash-rel/main/hypothetical.js", "../",
               Some("resolve/mod-js-slash-rel/main.js"), &cjs);

    // absolute paths

    let ctx = "resolve/subdir/hypothetical.js";
    let mut path = fixture_path();
    path.push("resolve/named-js");
    assert_resolves(ctx, path.to_str().unwrap(),
               Some("resolve/named-js.js"), &cjs);

    // modules

    let ctx = "resolve/hypothetical.js";
    assert_resolves(ctx,          "n-named-noext",
        Some("resolve/node_modules/n-named-noext"), &cjs);
    assert_resolves(ctx,          "n-named-js.js",
        Some("resolve/node_modules/n-named-js.js"), &cjs);
    assert_resolves(ctx,          "n-named-json.json",
        Some("resolve/node_modules/n-named-json.json"), &cjs);
    assert_resolves(ctx,          "n-named-mjs.mjs",
        Some("resolve/node_modules/n-named-mjs.mjs"), &esm);
    assert_resolves(ctx,          "n-named-jsz.jsz",
        Some("resolve/node_modules/n-named-jsz.jsz"), &cjs);

    assert_resolves(ctx,          "n-named-js",
        Some("resolve/node_modules/n-named-js.js"), &cjs);
    assert_resolves(ctx,          "n-named-json",
        Some("resolve/node_modules/n-named-json.json"), &cjs);
    assert_resolves(ctx,          "n-named-mjs",
        Some("resolve/node_modules/n-named-mjs.mjs"), &esm);

    assert_resolves(ctx,          "n-dir-js",
        Some("resolve/node_modules/n-dir-js/index.js"), &cjs);
    assert_resolves(ctx,          "n-dir-js/index",
        Some("resolve/node_modules/n-dir-js/index.js"), &cjs);
    assert_resolves(ctx,          "n-dir-json",
        Some("resolve/node_modules/n-dir-json/index.json"), &cjs);
    assert_resolves(ctx,          "n-dir-json/index",
        Some("resolve/node_modules/n-dir-json/index.json"), &cjs);
    assert_resolves(ctx,          "n-dir-mjs",
        Some("resolve/node_modules/n-dir-mjs/index.mjs"), &esm);
    assert_resolves(ctx,          "n-dir-mjs/index",
        Some("resolve/node_modules/n-dir-mjs/index.mjs"), &esm);

    assert_resolves(ctx,          "n-mod-noext-bare",
        Some("resolve/node_modules/n-mod-noext-bare/main-noext"), &cjs);
    assert_resolves(ctx,          "n-mod-noext-rel",
        Some("resolve/node_modules/n-mod-noext-rel/main-noext"), &cjs);

    assert_resolves(ctx,          "n-mod-main-nesting-bare",
        Some("resolve/node_modules/n-mod-main-nesting-bare/subdir/index.js"), &cjs);
    assert_resolves(ctx,          "n-mod-main-nesting-bare/subdir",
        Some("resolve/node_modules/n-mod-main-nesting-bare/subdir/inner-main.js"), &cjs);
    assert_resolves(ctx,          "n-mod-main-nesting-rel",
        Some("resolve/node_modules/n-mod-main-nesting-rel/subdir/index.js"), &cjs);
    assert_resolves(ctx,          "n-mod-main-nesting-rel/subdir",
        Some("resolve/node_modules/n-mod-main-nesting-rel/subdir/inner-main.js"), &cjs);

    assert_resolves(ctx,          "n-mod-js-ext-bare",
        Some("resolve/node_modules/n-mod-js-ext-bare/main-js.js"), &cjs);
    assert_resolves(ctx,          "n-mod-js-ext-rel",
        Some("resolve/node_modules/n-mod-js-ext-rel/main-js.js"), &cjs);
    assert_resolves(ctx,          "n-mod-js-noext-bare",
        Some("resolve/node_modules/n-mod-js-noext-bare/main-js.js"), &cjs);
    assert_resolves(ctx,          "n-mod-js-noext-rel",
        Some("resolve/node_modules/n-mod-js-noext-rel/main-js.js"), &cjs);
    assert_resolves(ctx,          "n-mod-js-dir-bare",
        Some("resolve/node_modules/n-mod-js-dir-bare/main-js/index.js"), &cjs);
    assert_resolves(ctx,          "n-mod-js-dir-rel",
        Some("resolve/node_modules/n-mod-js-dir-rel/main-js/index.js"), &cjs);

    assert_resolves(ctx,          "n-mod-json-ext-bare",
        Some("resolve/node_modules/n-mod-json-ext-bare/main-json.json"), &cjs);
    assert_resolves(ctx,          "n-mod-json-ext-rel",
        Some("resolve/node_modules/n-mod-json-ext-rel/main-json.json"), &cjs);
    assert_resolves(ctx,          "n-mod-json-noext-bare",
        Some("resolve/node_modules/n-mod-json-noext-bare/main-json.json"), &cjs);
    assert_resolves(ctx,          "n-mod-json-noext-rel",
        Some("resolve/node_modules/n-mod-json-noext-rel/main-json.json"), &cjs);
    assert_resolves(ctx,          "n-mod-json-dir-bare",
        Some("resolve/node_modules/n-mod-json-dir-bare/main-json/index.json"), &cjs);
    assert_resolves(ctx,          "n-mod-json-dir-rel",
        Some("resolve/node_modules/n-mod-json-dir-rel/main-json/index.json"), &cjs);

    assert_resolves(ctx,          "n-mod-mjs-ext-bare",
        Some("resolve/node_modules/n-mod-mjs-ext-bare/main-mjs.mjs"), &esm);
    assert_resolves(ctx,          "n-mod-mjs-ext-rel",
        Some("resolve/node_modules/n-mod-mjs-ext-rel/main-mjs.mjs"), &esm);
    assert_resolves(ctx,          "n-mod-mjs-noext-bare",
        Some("resolve/node_modules/n-mod-mjs-noext-bare/main-mjs.mjs"), &esm);
    assert_resolves(ctx,          "n-mod-mjs-noext-rel",
        Some("resolve/node_modules/n-mod-mjs-noext-rel/main-mjs.mjs"), &esm);
    assert_resolves(ctx,          "n-mod-mjs-dir-bare",
        Some("resolve/node_modules/n-mod-mjs-dir-bare/main-mjs/index.mjs"), &esm);
    assert_resolves(ctx,          "n-mod-mjs-dir-rel",
        Some("resolve/node_modules/n-mod-mjs-dir-rel/main-mjs/index.mjs"), &esm);

    assert_resolves(ctx,          "n-mod-js-slash-bare",
        Some("resolve/node_modules/n-mod-js-slash-bare/main.js"), &cjs);
    assert_resolves(ctx,          "n-mod-js-slash-rel",
        Some("resolve/node_modules/n-mod-js-slash-rel/main.js"), &cjs);

    assert_resolves(ctx,          "n-named-jsz", None, &cjs);

    assert_resolves(ctx,          "n-file-and-dir",
        Some("resolve/node_modules/n-file-and-dir.js"), &cjs);
    assert_resolves(ctx,          "n-file-and-dir/",
        Some("resolve/node_modules/n-file-and-dir/index.js"), &cjs);
    assert_resolves(ctx,          "n-file-and-mod",
        Some("resolve/node_modules/n-file-and-mod.js"), &cjs);
    assert_resolves(ctx,          "n-file-and-mod/",
        Some("resolve/node_modules/n-file-and-mod/main.js"), &cjs);
    assert_resolves(ctx,          "n-dir-js/",
        Some("resolve/node_modules/n-dir-js/index.js"), &cjs);
    assert_resolves(ctx,          "n-mod-js-noext-rel/",
        Some("resolve/node_modules/n-mod-js-noext-rel/main-js.js"), &cjs);
    assert_resolves(ctx,          "n-named-js.js/", None, &cjs);
    assert_resolves(ctx,          "n-named-js/", None, &cjs);
    assert_resolves(ctx,          "n-named-noext/", None, &cjs);

    assert_resolves(ctx,          "./n-named-noext", None, &cjs);
    assert_resolves(ctx,          "./n-named-js.js", None, &cjs);
    assert_resolves(ctx,          "./n-named-json.json", None, &cjs);
    assert_resolves(ctx,          "./n-named-mjs.mjs", None, &esm);
    assert_resolves(ctx,          "./n-named-jsz.jsz", None, &cjs);

    assert_resolves(ctx,          "./n-named-js", None, &cjs);
    assert_resolves(ctx,          "./n-named-json", None, &cjs);
    assert_resolves(ctx,          "./n-named-mjs", None, &esm);

    assert_resolves(ctx,          "./n-dir-js", None, &cjs);
    assert_resolves(ctx,          "./n-dir-js/index", None, &cjs);
    assert_resolves(ctx,          "./n-dir-json", None, &cjs);
    assert_resolves(ctx,          "./n-dir-json/index", None, &cjs);
    assert_resolves(ctx,          "./n-dir-mjs", None, &esm);
    assert_resolves(ctx,          "./n-dir-mjs/index", None, &esm);

    assert_resolves(ctx,          "./n-mod-noext-bare", None, &cjs);
    assert_resolves(ctx,          "./n-mod-noext-rel", None, &cjs);

    assert_resolves(ctx,          "./n-mod-main-nesting-bare", None, &cjs);
    assert_resolves(ctx,          "./n-mod-main-nesting-bare/subdir", None, &cjs);
    assert_resolves(ctx,          "./n-mod-main-nesting-rel", None, &cjs);
    assert_resolves(ctx,          "./n-mod-main-nesting-rel/subdir", None, &cjs);

    assert_resolves(ctx,          "./n-mod-js-ext-bare", None, &cjs);
    assert_resolves(ctx,          "./n-mod-js-ext-rel", None, &cjs);
    assert_resolves(ctx,          "./n-mod-js-noext-bare", None, &cjs);
    assert_resolves(ctx,          "./n-mod-js-noext-rel", None, &cjs);
    assert_resolves(ctx,          "./n-mod-js-dir-bare", None, &cjs);
    assert_resolves(ctx,          "./n-mod-js-dir-rel", None, &cjs);

    assert_resolves(ctx,          "./n-mod-json-ext-bare", None, &cjs);
    assert_resolves(ctx,          "./n-mod-json-ext-rel", None, &cjs);
    assert_resolves(ctx,          "./n-mod-json-noext-bare", None, &cjs);
    assert_resolves(ctx,          "./n-mod-json-noext-rel", None, &cjs);
    assert_resolves(ctx,          "./n-mod-json-dir-bare", None, &cjs);
    assert_resolves(ctx,          "./n-mod-json-dir-rel", None, &cjs);

    assert_resolves(ctx,          "./n-mod-mjs-ext-bare", None, &esm);
    assert_resolves(ctx,          "./n-mod-mjs-ext-rel", None, &esm);
    assert_resolves(ctx,          "./n-mod-mjs-noext-bare", None, &esm);
    assert_resolves(ctx,          "./n-mod-mjs-noext-rel", None, &esm);
    assert_resolves(ctx,          "./n-mod-mjs-dir-bare", None, &esm);
    assert_resolves(ctx,          "./n-mod-mjs-dir-rel", None, &esm);

    assert_resolves(ctx,          "./n-mod-js-slash-bare", None, &cjs);
    assert_resolves(ctx,          "./n-mod-js-slash-rel", None, &cjs);

    assert_resolves(ctx,          "./n-named-jsz", None, &cjs);

    assert_resolves(ctx,          "./n-file-and-dir", None, &cjs);
    assert_resolves(ctx,          "./n-file-and-dir/", None, &cjs);
    assert_resolves(ctx,          "./n-file-and-mod", None, &cjs);
    assert_resolves(ctx,          "./n-file-and-mod/", None, &cjs);
    assert_resolves(ctx,          "./n-dir-js/", None, &cjs);
    assert_resolves(ctx,          "./n-mod-js-noext-rel/", None, &cjs);
    assert_resolves(ctx,          "./n-named-js.js/", None, &cjs);
    assert_resolves(ctx,          "./n-named-js/", None, &cjs);
    assert_resolves(ctx,          "./n-named-noext/", None, &cjs);

    assert_resolves(ctx,          "shadowed",
        Some("resolve/node_modules/shadowed/index.js"), &cjs);

    assert_resolves(ctx,          "@user/scoped",
        Some("resolve/node_modules/@user/scoped/index.js"), &cjs);
    assert_resolves(ctx,          "@user/scoped/index",
        Some("resolve/node_modules/@user/scoped/index.js"), &cjs);
    assert_resolves(ctx,          "@user/scoped/index.js",
        Some("resolve/node_modules/@user/scoped/index.js"), &cjs);

    assert_resolves(ctx,          "shallow/s-named-noext",
        Some("resolve/node_modules/shallow/s-named-noext"), &cjs);
    assert_resolves(ctx,          "shallow/s-named-js.js",
        Some("resolve/node_modules/shallow/s-named-js.js"), &cjs);
    assert_resolves(ctx,          "shallow/s-named-json.json",
        Some("resolve/node_modules/shallow/s-named-json.json"), &cjs);
    assert_resolves(ctx,          "shallow/s-named-mjs.mjs",
        Some("resolve/node_modules/shallow/s-named-mjs.mjs"), &esm);
    assert_resolves(ctx,          "shallow/s-named-jsz.jsz",
        Some("resolve/node_modules/shallow/s-named-jsz.jsz"), &cjs);

    assert_resolves(ctx,          "shallow/s-named-js",
        Some("resolve/node_modules/shallow/s-named-js.js"), &cjs);
    assert_resolves(ctx,          "shallow/s-named-json",
        Some("resolve/node_modules/shallow/s-named-json.json"), &cjs);
    assert_resolves(ctx,          "shallow/s-named-mjs",
        Some("resolve/node_modules/shallow/s-named-mjs.mjs"), &esm);

    assert_resolves(ctx,          "shallow/s-dir-js",
        Some("resolve/node_modules/shallow/s-dir-js/index.js"), &cjs);
    assert_resolves(ctx,          "shallow/s-dir-js/index",
        Some("resolve/node_modules/shallow/s-dir-js/index.js"), &cjs);
    assert_resolves(ctx,          "shallow/s-dir-json",
        Some("resolve/node_modules/shallow/s-dir-json/index.json"), &cjs);
    assert_resolves(ctx,          "shallow/s-dir-json/index",
        Some("resolve/node_modules/shallow/s-dir-json/index.json"), &cjs);
    assert_resolves(ctx,          "shallow/s-dir-mjs",
        Some("resolve/node_modules/shallow/s-dir-mjs/index.mjs"), &esm);
    assert_resolves(ctx,          "shallow/s-dir-mjs/index",
        Some("resolve/node_modules/shallow/s-dir-mjs/index.mjs"), &esm);

    assert_resolves(ctx,          "shallow/s-mod-noext-bare",
        Some("resolve/node_modules/shallow/s-mod-noext-bare/main-noext"), &cjs);
    assert_resolves(ctx,          "shallow/s-mod-noext-rel",
        Some("resolve/node_modules/shallow/s-mod-noext-rel/main-noext"), &cjs);

    assert_resolves(ctx,          "shallow/s-mod-main-nesting-bare",
        Some("resolve/node_modules/shallow/s-mod-main-nesting-bare/subdir/index.js"), &cjs);
    assert_resolves(ctx,          "shallow/s-mod-main-nesting-bare/subdir",
        Some("resolve/node_modules/shallow/s-mod-main-nesting-bare/subdir/inner-main.js"), &cjs);
    assert_resolves(ctx,          "shallow/s-mod-main-nesting-rel",
        Some("resolve/node_modules/shallow/s-mod-main-nesting-rel/subdir/index.js"), &cjs);
    assert_resolves(ctx,          "shallow/s-mod-main-nesting-rel/subdir",
        Some("resolve/node_modules/shallow/s-mod-main-nesting-rel/subdir/inner-main.js"), &cjs);

    assert_resolves(ctx,          "shallow/s-mod-js-ext-bare",
        Some("resolve/node_modules/shallow/s-mod-js-ext-bare/main-js.js"), &cjs);
    assert_resolves(ctx,          "shallow/s-mod-js-ext-rel",
        Some("resolve/node_modules/shallow/s-mod-js-ext-rel/main-js.js"), &cjs);
    assert_resolves(ctx,          "shallow/s-mod-js-noext-bare",
        Some("resolve/node_modules/shallow/s-mod-js-noext-bare/main-js.js"), &cjs);
    assert_resolves(ctx,          "shallow/s-mod-js-noext-rel",
        Some("resolve/node_modules/shallow/s-mod-js-noext-rel/main-js.js"), &cjs);
    assert_resolves(ctx,          "shallow/s-mod-js-dir-bare",
        Some("resolve/node_modules/shallow/s-mod-js-dir-bare/main-js/index.js"), &cjs);
    assert_resolves(ctx,          "shallow/s-mod-js-dir-rel",
        Some("resolve/node_modules/shallow/s-mod-js-dir-rel/main-js/index.js"), &cjs);

    assert_resolves(ctx,          "shallow/s-mod-json-ext-bare",
        Some("resolve/node_modules/shallow/s-mod-json-ext-bare/main-json.json"), &cjs);
    assert_resolves(ctx,          "shallow/s-mod-json-ext-rel",
        Some("resolve/node_modules/shallow/s-mod-json-ext-rel/main-json.json"), &cjs);
    assert_resolves(ctx,          "shallow/s-mod-json-noext-bare",
        Some("resolve/node_modules/shallow/s-mod-json-noext-bare/main-json.json"), &cjs);
    assert_resolves(ctx,          "shallow/s-mod-json-noext-rel",
        Some("resolve/node_modules/shallow/s-mod-json-noext-rel/main-json.json"), &cjs);
    assert_resolves(ctx,          "shallow/s-mod-json-dir-bare",
        Some("resolve/node_modules/shallow/s-mod-json-dir-bare/main-json/index.json"), &cjs);
    assert_resolves(ctx,          "shallow/s-mod-json-dir-rel",
        Some("resolve/node_modules/shallow/s-mod-json-dir-rel/main-json/index.json"), &cjs);

    assert_resolves(ctx,          "shallow/s-mod-mjs-ext-bare",
        Some("resolve/node_modules/shallow/s-mod-mjs-ext-bare/main-mjs.mjs"), &esm);
    assert_resolves(ctx,          "shallow/s-mod-mjs-ext-rel",
        Some("resolve/node_modules/shallow/s-mod-mjs-ext-rel/main-mjs.mjs"), &esm);
    assert_resolves(ctx,          "shallow/s-mod-mjs-noext-bare",
        Some("resolve/node_modules/shallow/s-mod-mjs-noext-bare/main-mjs.mjs"), &esm);
    assert_resolves(ctx,          "shallow/s-mod-mjs-noext-rel",
        Some("resolve/node_modules/shallow/s-mod-mjs-noext-rel/main-mjs.mjs"), &esm);
    assert_resolves(ctx,          "shallow/s-mod-mjs-dir-bare",
        Some("resolve/node_modules/shallow/s-mod-mjs-dir-bare/main-mjs/index.mjs"), &esm);
    assert_resolves(ctx,          "shallow/s-mod-mjs-dir-rel",
        Some("resolve/node_modules/shallow/s-mod-mjs-dir-rel/main-mjs/index.mjs"), &esm);

    assert_resolves(ctx,          "shallow/s-mod-js-slash-bare",
        Some("resolve/node_modules/shallow/s-mod-js-slash-bare/main.js"), &cjs);
    assert_resolves(ctx,          "shallow/s-mod-js-slash-rel",
        Some("resolve/node_modules/shallow/s-mod-js-slash-rel/main.js"), &cjs);

    assert_resolves(ctx,          "shallow/s-named-jsz", None, &cjs);

    assert_resolves(ctx,          "shallow/s-file-and-dir",
        Some("resolve/node_modules/shallow/s-file-and-dir.js"), &cjs);
    assert_resolves(ctx,          "shallow/s-file-and-dir/",
        Some("resolve/node_modules/shallow/s-file-and-dir/index.js"), &cjs);
    assert_resolves(ctx,          "shallow/s-file-and-mod",
        Some("resolve/node_modules/shallow/s-file-and-mod.js"), &cjs);
    assert_resolves(ctx,          "shallow/s-file-and-mod/",
        Some("resolve/node_modules/shallow/s-file-and-mod/main.js"), &cjs);
    assert_resolves(ctx,          "shallow/s-dir-js/",
        Some("resolve/node_modules/shallow/s-dir-js/index.js"), &cjs);
    assert_resolves(ctx,          "shallow/s-mod-js-noext-rel/",
        Some("resolve/node_modules/shallow/s-mod-js-noext-rel/main-js.js"), &cjs);
    assert_resolves(ctx,          "shallow/s-named-js.js/", None, &cjs);
    assert_resolves(ctx,          "shallow/s-named-js/", None, &cjs);
    assert_resolves(ctx,          "shallow/s-named-noext/", None, &cjs);

    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-named-noext",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-named-noext"), &cjs);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-named-js.js",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-named-js.js"), &cjs);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-named-json.json",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-named-json.json"), &cjs);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-named-mjs.mjs",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-named-mjs.mjs"), &esm);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-named-jsz.jsz",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-named-jsz.jsz"), &cjs);

    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-named-js",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-named-js.js"), &cjs);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-named-json",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-named-json.json"), &cjs);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-named-mjs",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-named-mjs.mjs"), &esm);

    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-dir-js",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-dir-js/index.js"), &cjs);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-dir-js/index",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-dir-js/index.js"), &cjs);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-dir-json",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-dir-json/index.json"), &cjs);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-dir-json/index",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-dir-json/index.json"), &cjs);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-dir-mjs",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-dir-mjs/index.mjs"), &esm);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-dir-mjs/index",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-dir-mjs/index.mjs"), &esm);

    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-mod-noext-bare",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-noext-bare/main-noext"), &cjs);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-mod-noext-rel",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-noext-rel/main-noext"), &cjs);

    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-mod-main-nesting-bare",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-main-nesting-bare/subdir/index.js"), &cjs);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-mod-main-nesting-bare/subdir",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-main-nesting-bare/subdir/inner-main.js"), &cjs);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-mod-main-nesting-rel",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-main-nesting-rel/subdir/index.js"), &cjs);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-mod-main-nesting-rel/subdir",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-main-nesting-rel/subdir/inner-main.js"), &cjs);

    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-mod-js-ext-bare",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-js-ext-bare/main-js.js"), &cjs);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-mod-js-ext-rel",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-js-ext-rel/main-js.js"), &cjs);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-mod-js-noext-bare",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-js-noext-bare/main-js.js"), &cjs);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-mod-js-noext-rel",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-js-noext-rel/main-js.js"), &cjs);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-mod-js-dir-bare",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-js-dir-bare/main-js/index.js"), &cjs);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-mod-js-dir-rel",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-js-dir-rel/main-js/index.js"), &cjs);

    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-mod-json-ext-bare",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-json-ext-bare/main-json.json"), &cjs);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-mod-json-ext-rel",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-json-ext-rel/main-json.json"), &cjs);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-mod-json-noext-bare",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-json-noext-bare/main-json.json"), &cjs);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-mod-json-noext-rel",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-json-noext-rel/main-json.json"), &cjs);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-mod-json-dir-bare",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-json-dir-bare/main-json/index.json"), &cjs);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-mod-json-dir-rel",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-json-dir-rel/main-json/index.json"), &cjs);

    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-mod-mjs-ext-bare",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-mjs-ext-bare/main-mjs.mjs"), &esm);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-mod-mjs-ext-rel",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-mjs-ext-rel/main-mjs.mjs"), &esm);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-mod-mjs-noext-bare",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-mjs-noext-bare/main-mjs.mjs"), &esm);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-mod-mjs-noext-rel",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-mjs-noext-rel/main-mjs.mjs"), &esm);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-mod-mjs-dir-bare",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-mjs-dir-bare/main-mjs/index.mjs"), &esm);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-mod-mjs-dir-rel",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-mjs-dir-rel/main-mjs/index.mjs"), &esm);

    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-mod-js-slash-bare",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-js-slash-bare/main.js"), &cjs);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-mod-js-slash-rel",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-js-slash-rel/main.js"), &cjs);

    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-named-jsz", None, &cjs);

    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-file-and-dir",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-file-and-dir.js"), &cjs);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-file-and-dir/",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-file-and-dir/index.js"), &cjs);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-file-and-mod",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-file-and-mod.js"), &cjs);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-file-and-mod/",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-file-and-mod/main.js"), &cjs);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-dir-js/",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-dir-js/index.js"), &cjs);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-mod-js-noext-rel/",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-js-noext-rel/main-js.js"), &cjs);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-named-js.js/", None, &cjs);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-named-js/", None, &cjs);
    assert_resolves(ctx,          "deep/dir1/dir2/dir3/d-named-noext/", None, &cjs);

    let ctx = "resolve/subdir/hypothetical.js";
    assert_resolves(ctx,                 "shadowed",
         Some("resolve/subdir/node_modules/shadowed/index.js"), &cjs);

    let ctx = "resolve/subdir/subdir2/hypothetical.js";
    assert_resolves(ctx,                          "shadowed",
         Some("resolve/subdir/subdir2/node_modules/shadowed/index.js"), &cjs);

    let ctx = "resolve-order/hypothetical.js";
    assert_resolves(ctx,  "./1-file",
         Some("resolve-order/1-file"), &cjs);
    assert_resolves(ctx,  "./2-file",
         Some("resolve-order/2-file.js"), &cjs);
    assert_resolves(ctx,  "./3-file",
         Some("resolve-order/3-file.json"), &cjs);
    assert_resolves(ctx,  "./1-dir",
         Some("resolve-order/1-dir.js"), &cjs);
    assert_resolves(ctx,  "./2-dir",
         Some("resolve-order/2-dir.json"), &cjs);
    assert_resolves(ctx,  "./3-dir",
         Some("resolve-order/3-dir/index.js"), &cjs);
    assert_resolves(ctx,  "./4-dir",
         Some("resolve-order/4-dir/index.json"), &cjs);
    assert_resolves(ctx,  "./1-dir/",
         Some("resolve-order/1-dir/index.js"), &cjs);
    assert_resolves(ctx,  "./2-dir/",
         Some("resolve-order/2-dir/index.js"), &cjs);
    assert_resolves(ctx,  "./3-dir/",
         Some("resolve-order/3-dir/index.js"), &cjs);
    assert_resolves(ctx,  "./4-dir/",
         Some("resolve-order/4-dir/index.json"), &cjs);
}

#[test]
fn test_resolve_consistency() {
    // meta-test: ensure test_resolve matches node behavior

    type Cases = FnvHashSet<(String, Option<String>)>;
    type CaseMap = FnvHashMap<String, Cases>;

    let mut cjs = FnvHashMap::default();
    let mut esm = FnvHashMap::default();

    {
        let append = |ctx: &str, from: &str, to: Option<&str>, input_options: &InputOptions| {
            let assertions = if input_options.es6_syntax {
                &mut esm
            } else {
                &mut cjs
            };
            assertions.entry(ctx.to_owned())
                .or_insert_with(FnvHashSet::default)
                .insert((from.to_owned(), to.map(ToOwned::to_owned)));
        };

        test_resolve_with(append);
    }

    fn make_source(base: &Path, cases: &Cases) -> Vec<u8> {
        let mut b = indoc!(br#"
            'use strict'
            const assert = require('assert').strict
            const path = require('path')
            const fs = require('fs')
            function n(from) {
                let fail = false
                try {{require.resolve(from), fail = true}} catch(_) {{}}
                if (fail) assert.fail(`'${from}' does not fail to resolve`)
            }
            function y(from, to) {
                try {
                    assert.equal(fs.realpathSync(require.resolve(from)), fs.realpathSync(to))
                } catch (e) {
                    assert.fail(`'${from}' does not resolve to '${to}'`)
                }
            }
        "#).to_vec();
        for (from, to) in cases {
            let from_path = Path::new(from);
            let from = if from_path.is_absolute() {
                let suffix = from_path.strip_prefix(fixture_path()).expect("absolute path outside of fixtures");
                serde_json::to_string(&base.join(suffix))
            } else {
                serde_json::to_string(from)
            }.unwrap();
            if let Some(to) = to {
                let mut to_path = base.to_owned();
                to_path.append_resolving(to);
                let to = serde_json::to_string(to_path.to_str().unwrap()).unwrap();
                writeln!(b, "y({from}, {to})", from=from, to=to).unwrap();
            } else {
                writeln!(b, "n({from})", from=from).unwrap();
            }
        }
        // io::stdout().write_all(&b).unwrap();
        b
    }
    fn test_file(base: &Path, esm: bool, ctx: &str, cases: &Cases) {
        let mut ctx_dir = base.to_owned();
        ctx_dir.append_resolving(ctx);
        ctx_dir.pop();
        // let ext = if esm { ".mjs" } else { ".js" };
        let ext = ".js";

        let mut file = tempfile::Builder::new()
            .suffix(ext)
            .tempfile_in(ctx_dir)
            .unwrap();
        file.as_file_mut()
            .write_all(&make_source(base, cases))
            .unwrap();

        let mut args = Vec::new();
        if esm {
            args.push("--experimental-modules");
        }
        args.push(file.path().to_str().unwrap());
        let output = process::Command::new("node")
            .args(&args)
            .output()
            .expect("failed to run node");

        if !output.status.success() {
            io::stderr().write(&output.stderr).unwrap();
            assert!(false);
        }
    }
    fn test_file_map(base: &Path, esm: bool, map: &CaseMap) {
        for (ctx, cases) in map.into_iter() {
            test_file(base, esm, ctx, cases)
        }
    }

    let base_dir = tempfile::tempdir().unwrap();
    let fixture_dir = fixture_path();
    for entry in WalkDir::new(&fixture_dir)
        .into_iter()
        .filter_map(Result::ok) {
        let local_path = entry.path().strip_prefix(&fixture_dir).unwrap();
        if local_path.components().next().is_none() { continue }

        let new_path = base_dir.path().join(local_path);
        // println!("{} {}", entry.path().display(), new_path.display());
        if entry.file_type().is_dir() {
            fs::create_dir(new_path).unwrap();
        } else {
            fs::copy(entry.path(), new_path).unwrap();
        }
    }
    test_file_map(base_dir.path(), false, &cjs);
    test_file_map(base_dir.path(), true, &esm);
}

#[test]
fn test_external() {
    fn assert_resolves(context: &str, from: &str, to: Resolved, input_options: &InputOptions) {
        let base_path = fixture_path();
        let to = match to {
            Resolved::Normal(mut path) => {
                path.prepend_resolving(&base_path);
                Resolved::Normal(path)
            }
            r => r,
        };
        let mut context_path = base_path;
        context_path.append_resolving(context);

        let resolver = Resolver::new(input_options.clone());
        // resolves with an empty cache...
        assert_eq!(resolver.resolve(&context_path, from).unwrap(), to);
        // ...and with everything cached
        assert_eq!(resolver.resolve(&context_path, from).unwrap(), to);
    }

    let ext = InputOptions {
        for_browser: false,
        es6_syntax: false,
        es6_syntax_everywhere: false,
        external: vec![
            "external".to_owned(),
            "external-only-module".to_owned(),
        ].into_iter().collect(),
    };
    let non = InputOptions {
        for_browser: false,
        es6_syntax: false,
        es6_syntax_everywhere: false,
        external: Default::default(),
    };

    let ctx = "resolve/hypothetical.js";
    assert_resolves(ctx, "external", Resolved::External, &ext);
    assert_resolves(ctx, "external/", Resolved::External, &ext);
    assert_resolves(ctx, "external/file.js", Resolved::External, &ext);
    assert_resolves(ctx, "external/file", Resolved::External, &ext);
    assert_resolves(ctx, "external/subdir", Resolved::External, &ext);
    assert_resolves(ctx, "external/subdir/", Resolved::External, &ext);
    assert_resolves(ctx, "external/subdir/index.js", Resolved::External, &ext);
    assert_resolves(ctx,                     "./external",
        Resolved::Normal(PathBuf::from("resolve/external.js")), &ext);

    assert_resolves(ctx,                     "./external",
        Resolved::Normal(PathBuf::from("resolve/external.js")), &non);
    assert_resolves(ctx,                                    "external",
        Resolved::Normal(PathBuf::from("resolve/node_modules/external/index.js")), &non);
    assert_resolves(ctx,                                    "external/",
        Resolved::Normal(PathBuf::from("resolve/node_modules/external/index.js")), &non);
    assert_resolves(ctx,                                    "external/file.js",
        Resolved::Normal(PathBuf::from("resolve/node_modules/external/file.js")), &non);
    assert_resolves(ctx,                                    "external/file",
        Resolved::Normal(PathBuf::from("resolve/node_modules/external/file.js")), &non);
    assert_resolves(ctx,                                    "external/subdir",
        Resolved::Normal(PathBuf::from("resolve/node_modules/external/subdir/index.js")), &non);
    assert_resolves(ctx,                                    "external/subdir/index",
        Resolved::Normal(PathBuf::from("resolve/node_modules/external/subdir/index.js")), &non);
    assert_resolves(ctx,                                    "external/subdir/index.js",
        Resolved::Normal(PathBuf::from("resolve/node_modules/external/subdir/index.js")), &non);
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
