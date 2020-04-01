use super::*;
use crate::input_options::PackageManager;
use crate::path_ext::*;
use fnv::{FnvHashMap, FnvHashSet};
use indoc::indoc;
use matches::assert_matches;
use serde_json;
use std::io::{self, Write};
use std::path::Path;
use std::{fs, process};
use walkdir::WalkDir;

fn fixture_path() -> PathBuf {
    let mut path = std::env::current_dir().unwrap();
    path.push("fixtures");
    path
}

#[test]
fn test_resolve_path_or_module() {
    fn path_resolves(from: &str, to: Option<&str>) {
        let base_path = fixture_path();
        let to_path = to.map(|to| {
            let mut to_path = base_path.clone();
            to_path.append_resolving(to);
            to_path
        });
        let mut from_path = base_path;
        from_path.append_resolving(from);

        let resolver = Resolver::new(InputOptions::default());
        let expected = to_path.map(Resolved::Normal);
        // resolves with an empty cache...
        assert_eq!(
            resolver
                .resolve_path_or_module(None, from_path.clone(), false, false)
                .unwrap(),
            expected
        );
        // ...and with everything cached
        assert_eq!(
            resolver
                .resolve_path_or_module(None, from_path, false, false)
                .unwrap(),
            expected
        );
    }
    path_resolves("resolve/named-noext", Some("resolve/named-noext"));
    path_resolves("resolve/named-js.js", Some("resolve/named-js.js"));
    path_resolves("resolve/named-json.json", Some("resolve/named-json.json"));
    path_resolves("resolve/named-mjs.mjs", Some("resolve/named-mjs.mjs"));
    path_resolves("resolve/named-jsz.jsz", Some("resolve/named-jsz.jsz"));

    path_resolves("resolve/named-js", Some("resolve/named-js.js"));
    path_resolves("resolve/named-json", Some("resolve/named-json.json"));
    path_resolves("resolve/named-mjs", Some("resolve/named-mjs.mjs"));

    path_resolves("resolve/dir-js", Some("resolve/dir-js/index.js"));
    path_resolves("resolve/dir-js/index", Some("resolve/dir-js/index.js"));
    path_resolves("resolve/dir-json", Some("resolve/dir-json/index.json"));
    path_resolves(
        "resolve/dir-json/index",
        Some("resolve/dir-json/index.json"),
    );
    path_resolves("resolve/dir-mjs", Some("resolve/dir-mjs/index.mjs"));
    path_resolves("resolve/dir-mjs/index", Some("resolve/dir-mjs/index.mjs"));

    path_resolves(
        "resolve/mod-noext-bare",
        Some("resolve/mod-noext-bare/main-noext"),
    );
    path_resolves(
        "resolve/mod-noext-rel",
        Some("resolve/mod-noext-rel/main-noext"),
    );

    path_resolves(
        "resolve/mod-main-nesting-bare",
        Some("resolve/mod-main-nesting-bare/subdir/index.js"),
    );
    path_resolves(
        "resolve/mod-main-nesting-bare/subdir",
        Some("resolve/mod-main-nesting-bare/subdir/inner-main.js"),
    );
    path_resolves(
        "resolve/mod-main-nesting-rel",
        Some("resolve/mod-main-nesting-rel/subdir/index.js"),
    );
    path_resolves(
        "resolve/mod-main-nesting-rel/subdir",
        Some("resolve/mod-main-nesting-rel/subdir/inner-main.js"),
    );

    path_resolves(
        "resolve/mod-js-ext-bare",
        Some("resolve/mod-js-ext-bare/main-js.js"),
    );
    path_resolves(
        "resolve/mod-js-ext-rel",
        Some("resolve/mod-js-ext-rel/main-js.js"),
    );
    path_resolves(
        "resolve/mod-js-noext-bare",
        Some("resolve/mod-js-noext-bare/main-js.js"),
    );
    path_resolves(
        "resolve/mod-js-noext-rel",
        Some("resolve/mod-js-noext-rel/main-js.js"),
    );
    path_resolves(
        "resolve/mod-js-dir-bare",
        Some("resolve/mod-js-dir-bare/main-js/index.js"),
    );
    path_resolves(
        "resolve/mod-js-dir-rel",
        Some("resolve/mod-js-dir-rel/main-js/index.js"),
    );

    path_resolves(
        "resolve/mod-json-ext-bare",
        Some("resolve/mod-json-ext-bare/main-json.json"),
    );
    path_resolves(
        "resolve/mod-json-ext-rel",
        Some("resolve/mod-json-ext-rel/main-json.json"),
    );
    path_resolves(
        "resolve/mod-json-noext-bare",
        Some("resolve/mod-json-noext-bare/main-json.json"),
    );
    path_resolves(
        "resolve/mod-json-noext-rel",
        Some("resolve/mod-json-noext-rel/main-json.json"),
    );
    path_resolves(
        "resolve/mod-json-dir-bare",
        Some("resolve/mod-json-dir-bare/main-json/index.json"),
    );
    path_resolves(
        "resolve/mod-json-dir-rel",
        Some("resolve/mod-json-dir-rel/main-json/index.json"),
    );

    path_resolves(
        "resolve/mod-mjs-ext-bare",
        Some("resolve/mod-mjs-ext-bare/main-mjs.mjs"),
    );
    path_resolves(
        "resolve/mod-mjs-ext-rel",
        Some("resolve/mod-mjs-ext-rel/main-mjs.mjs"),
    );
    path_resolves(
        "resolve/mod-mjs-noext-bare",
        Some("resolve/mod-mjs-noext-bare/main-mjs.mjs"),
    );
    path_resolves(
        "resolve/mod-mjs-noext-rel",
        Some("resolve/mod-mjs-noext-rel/main-mjs.mjs"),
    );
    path_resolves(
        "resolve/mod-mjs-dir-bare",
        Some("resolve/mod-mjs-dir-bare/main-mjs/index.mjs"),
    );
    path_resolves(
        "resolve/mod-mjs-dir-rel",
        Some("resolve/mod-mjs-dir-rel/main-mjs/index.mjs"),
    );

    path_resolves("resolve/named-jsz", None);
}

fn assert_resolves_with_options(
    context: &str,
    from: &str,
    to: Option<&str>,
    options: Option<&InputOptions>,
) {
    fn fixture_path() -> PathBuf {
        let mut path = std::env::current_dir().unwrap();
        path.push("fixtures");
        path
    }

    let defaults = InputOptions::default();
    let options = match options {
        Some(options) => options,
        None => &defaults,
    };
    let base_path = fixture_path();
    let to_path = to.map(|to| {
        let mut to_path = base_path.clone();
        to_path.append_resolving(to);
        to_path
    });
    let mut context_path = base_path;
    context_path.append_resolving(context);

    let resolver = Resolver::new(options.clone());
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

fn assert_resolves(context: &str, from: &str, to: Option<&str>) {
    assert_resolves_with_options(context, from, to, None);
}

fn assert_resolves_bower(context: &str, from: &str, to: Option<&str>) {
    let external = FnvHashSet::default();
    let input_options = InputOptions {
        package_manager: PackageManager::Bower,
        external,
    };
    assert_resolves_with_options(context, from, to, Some(&input_options));
}

fn test_resolve_with<F>(mut assert_resolves: F)
where
    F: FnMut(&str, &str, Option<&str>),
{
    // relative paths

    let ctx = "resolve/hypothetical.js";
    assert_resolves(ctx, "./named-noext", Some("resolve/named-noext"));
    assert_resolves(ctx, "./named-js.js", Some("resolve/named-js.js"));
    assert_resolves(ctx, "./named-json.json", Some("resolve/named-json.json"));
    assert_resolves(ctx, "./named-mjs.mjs", Some("resolve/named-mjs.mjs"));
    assert_resolves(ctx, "./named-jsz.jsz", Some("resolve/named-jsz.jsz"));

    assert_resolves(ctx, "./named-js", Some("resolve/named-js.js"));
    assert_resolves(ctx, "./named-json", Some("resolve/named-json.json"));
    assert_resolves(ctx, "./named-mjs", Some("resolve/named-mjs.mjs"));

    assert_resolves(ctx, "./dir-js", Some("resolve/dir-js/index.js"));
    assert_resolves(ctx, "./dir-js/index", Some("resolve/dir-js/index.js"));
    assert_resolves(ctx, "./dir-json", Some("resolve/dir-json/index.json"));
    assert_resolves(ctx, "./dir-json/index", Some("resolve/dir-json/index.json"));
    assert_resolves(ctx, "./dir-mjs", Some("resolve/dir-mjs/index.mjs"));
    assert_resolves(ctx, "./dir-mjs/index", Some("resolve/dir-mjs/index.mjs"));

    assert_resolves(
        ctx,
        "./mod-noext-bare",
        Some("resolve/mod-noext-bare/main-noext"),
    );
    assert_resolves(
        ctx,
        "./mod-noext-rel",
        Some("resolve/mod-noext-rel/main-noext"),
    );

    assert_resolves(
        ctx,
        "./mod-main-nesting-bare",
        Some("resolve/mod-main-nesting-bare/subdir/index.js"),
    );
    assert_resolves(
        ctx,
        "./mod-main-nesting-bare/subdir",
        Some("resolve/mod-main-nesting-bare/subdir/inner-main.js"),
    );
    assert_resolves(
        ctx,
        "./mod-main-nesting-rel",
        Some("resolve/mod-main-nesting-rel/subdir/index.js"),
    );
    assert_resolves(
        ctx,
        "./mod-main-nesting-rel/subdir",
        Some("resolve/mod-main-nesting-rel/subdir/inner-main.js"),
    );

    assert_resolves(
        ctx,
        "./mod-js-ext-bare",
        Some("resolve/mod-js-ext-bare/main-js.js"),
    );
    assert_resolves(
        ctx,
        "./mod-js-ext-rel",
        Some("resolve/mod-js-ext-rel/main-js.js"),
    );
    assert_resolves(
        ctx,
        "./mod-js-noext-bare",
        Some("resolve/mod-js-noext-bare/main-js.js"),
    );
    assert_resolves(
        ctx,
        "./mod-js-noext-rel",
        Some("resolve/mod-js-noext-rel/main-js.js"),
    );
    assert_resolves(
        ctx,
        "./mod-js-dir-bare",
        Some("resolve/mod-js-dir-bare/main-js/index.js"),
    );
    assert_resolves(
        ctx,
        "./mod-js-dir-rel",
        Some("resolve/mod-js-dir-rel/main-js/index.js"),
    );

    assert_resolves(
        ctx,
        "./mod-json-ext-bare",
        Some("resolve/mod-json-ext-bare/main-json.json"),
    );
    assert_resolves(
        ctx,
        "./mod-json-ext-rel",
        Some("resolve/mod-json-ext-rel/main-json.json"),
    );
    assert_resolves(
        ctx,
        "./mod-json-noext-bare",
        Some("resolve/mod-json-noext-bare/main-json.json"),
    );
    assert_resolves(
        ctx,
        "./mod-json-noext-rel",
        Some("resolve/mod-json-noext-rel/main-json.json"),
    );
    assert_resolves(
        ctx,
        "./mod-json-dir-bare",
        Some("resolve/mod-json-dir-bare/main-json/index.json"),
    );
    assert_resolves(
        ctx,
        "./mod-json-dir-rel",
        Some("resolve/mod-json-dir-rel/main-json/index.json"),
    );

    assert_resolves(
        ctx,
        "./mod-mjs-ext-bare",
        Some("resolve/mod-mjs-ext-bare/main-mjs.mjs"),
    );
    assert_resolves(
        ctx,
        "./mod-mjs-ext-rel",
        Some("resolve/mod-mjs-ext-rel/main-mjs.mjs"),
    );
    assert_resolves(
        ctx,
        "./mod-mjs-noext-bare",
        Some("resolve/mod-mjs-noext-bare/main-mjs.mjs"),
    );
    assert_resolves(
        ctx,
        "./mod-mjs-noext-rel",
        Some("resolve/mod-mjs-noext-rel/main-mjs.mjs"),
    );
    assert_resolves(
        ctx,
        "./mod-mjs-dir-bare",
        Some("resolve/mod-mjs-dir-bare/main-mjs/index.mjs"),
    );
    assert_resolves(
        ctx,
        "./mod-mjs-dir-rel",
        Some("resolve/mod-mjs-dir-rel/main-mjs/index.mjs"),
    );

    assert_resolves(
        ctx,
        "./mod-js-slash-bare",
        Some("resolve/mod-js-slash-bare/main.js"),
    );
    assert_resolves(
        ctx,
        "./mod-js-slash-rel",
        Some("resolve/mod-js-slash-rel/main.js"),
    );

    assert_resolves(ctx, "./named-jsz", None);

    assert_resolves(ctx, "./file-and-dir", Some("resolve/file-and-dir.js"));
    assert_resolves(
        ctx,
        "./file-and-dir/",
        Some("resolve/file-and-dir/index.js"),
    );
    assert_resolves(ctx, "./file-and-mod", Some("resolve/file-and-mod.js"));
    assert_resolves(ctx, "./file-and-mod/", Some("resolve/file-and-mod/main.js"));
    assert_resolves(ctx, "./dir-js/", Some("resolve/dir-js/index.js"));
    assert_resolves(
        ctx,
        "./mod-js-noext-rel/",
        Some("resolve/mod-js-noext-rel/main-js.js"),
    );
    assert_resolves(ctx, "./named-js.js/", None);
    assert_resolves(ctx, "./named-js/", None);
    assert_resolves(ctx, "./named-noext/", None);

    let ctx = "resolve/subdir/hypothetical.js";
    assert_resolves(ctx, "./named-js", None);

    assert_resolves(ctx, "../named-noext", Some("resolve/named-noext"));
    assert_resolves(ctx, "../named-js.js", Some("resolve/named-js.js"));
    assert_resolves(ctx, "../named-json.json", Some("resolve/named-json.json"));
    assert_resolves(ctx, "../named-mjs.mjs", Some("resolve/named-mjs.mjs"));
    assert_resolves(ctx, "../named-jsz.jsz", Some("resolve/named-jsz.jsz"));

    assert_resolves(ctx, "../named-js", Some("resolve/named-js.js"));
    assert_resolves(ctx, "../named-json", Some("resolve/named-json.json"));
    assert_resolves(ctx, "../named-mjs", Some("resolve/named-mjs.mjs"));

    assert_resolves(ctx, "../dir-js", Some("resolve/dir-js/index.js"));
    assert_resolves(ctx, "../dir-js/index", Some("resolve/dir-js/index.js"));
    assert_resolves(ctx, "../dir-json", Some("resolve/dir-json/index.json"));
    assert_resolves(
        ctx,
        "../dir-json/index",
        Some("resolve/dir-json/index.json"),
    );
    assert_resolves(ctx, "../dir-mjs", Some("resolve/dir-mjs/index.mjs"));
    assert_resolves(ctx, "../dir-mjs/index", Some("resolve/dir-mjs/index.mjs"));

    assert_resolves(
        ctx,
        "../mod-noext-bare",
        Some("resolve/mod-noext-bare/main-noext"),
    );
    assert_resolves(
        ctx,
        "../mod-noext-rel",
        Some("resolve/mod-noext-rel/main-noext"),
    );

    assert_resolves(
        ctx,
        "../mod-main-nesting-bare",
        Some("resolve/mod-main-nesting-bare/subdir/index.js"),
    );
    assert_resolves(
        ctx,
        "../mod-main-nesting-bare/subdir",
        Some("resolve/mod-main-nesting-bare/subdir/inner-main.js"),
    );
    assert_resolves(
        ctx,
        "../mod-main-nesting-rel",
        Some("resolve/mod-main-nesting-rel/subdir/index.js"),
    );
    assert_resolves(
        ctx,
        "../mod-main-nesting-rel/subdir",
        Some("resolve/mod-main-nesting-rel/subdir/inner-main.js"),
    );

    assert_resolves(
        ctx,
        "../mod-js-ext-bare",
        Some("resolve/mod-js-ext-bare/main-js.js"),
    );
    assert_resolves(
        ctx,
        "../mod-js-ext-rel",
        Some("resolve/mod-js-ext-rel/main-js.js"),
    );
    assert_resolves(
        ctx,
        "../mod-js-noext-bare",
        Some("resolve/mod-js-noext-bare/main-js.js"),
    );
    assert_resolves(
        ctx,
        "../mod-js-noext-rel",
        Some("resolve/mod-js-noext-rel/main-js.js"),
    );
    assert_resolves(
        ctx,
        "../mod-js-dir-bare",
        Some("resolve/mod-js-dir-bare/main-js/index.js"),
    );
    assert_resolves(
        ctx,
        "../mod-js-dir-rel",
        Some("resolve/mod-js-dir-rel/main-js/index.js"),
    );

    assert_resolves(
        ctx,
        "../mod-json-ext-bare",
        Some("resolve/mod-json-ext-bare/main-json.json"),
    );
    assert_resolves(
        ctx,
        "../mod-json-ext-rel",
        Some("resolve/mod-json-ext-rel/main-json.json"),
    );
    assert_resolves(
        ctx,
        "../mod-json-noext-bare",
        Some("resolve/mod-json-noext-bare/main-json.json"),
    );
    assert_resolves(
        ctx,
        "../mod-json-noext-rel",
        Some("resolve/mod-json-noext-rel/main-json.json"),
    );
    assert_resolves(
        ctx,
        "../mod-json-dir-bare",
        Some("resolve/mod-json-dir-bare/main-json/index.json"),
    );
    assert_resolves(
        ctx,
        "../mod-json-dir-rel",
        Some("resolve/mod-json-dir-rel/main-json/index.json"),
    );

    assert_resolves(
        ctx,
        "../mod-mjs-ext-bare",
        Some("resolve/mod-mjs-ext-bare/main-mjs.mjs"),
    );
    assert_resolves(
        ctx,
        "../mod-mjs-ext-rel",
        Some("resolve/mod-mjs-ext-rel/main-mjs.mjs"),
    );
    assert_resolves(
        ctx,
        "../mod-mjs-noext-bare",
        Some("resolve/mod-mjs-noext-bare/main-mjs.mjs"),
    );
    assert_resolves(
        ctx,
        "../mod-mjs-noext-rel",
        Some("resolve/mod-mjs-noext-rel/main-mjs.mjs"),
    );
    assert_resolves(
        ctx,
        "../mod-mjs-dir-bare",
        Some("resolve/mod-mjs-dir-bare/main-mjs/index.mjs"),
    );
    assert_resolves(
        ctx,
        "../mod-mjs-dir-rel",
        Some("resolve/mod-mjs-dir-rel/main-mjs/index.mjs"),
    );

    assert_resolves(
        ctx,
        "../mod-js-slash-bare",
        Some("resolve/mod-js-slash-bare/main.js"),
    );
    assert_resolves(
        ctx,
        "../mod-js-slash-rel",
        Some("resolve/mod-js-slash-rel/main.js"),
    );

    assert_resolves(ctx, "../named-jsz", None);

    assert_resolves(ctx, "../file-and-dir", Some("resolve/file-and-dir.js"));
    assert_resolves(
        ctx,
        "../file-and-dir/",
        Some("resolve/file-and-dir/index.js"),
    );
    assert_resolves(ctx, "../file-and-mod", Some("resolve/file-and-mod.js"));
    assert_resolves(
        ctx,
        "../file-and-mod/",
        Some("resolve/file-and-mod/main.js"),
    );
    assert_resolves(ctx, "../dir-js/", Some("resolve/dir-js/index.js"));
    assert_resolves(
        ctx,
        "../mod-js-noext-rel/",
        Some("resolve/mod-js-noext-rel/main-js.js"),
    );
    assert_resolves(ctx, "../named-js.js/", None);
    assert_resolves(ctx, "../named-js/", None);
    assert_resolves(ctx, "../named-noext/", None);

    assert_resolves(
        ctx,
        "../mod-self-slash",
        Some("resolve/mod-self-slash/index.js"),
    );
    assert_resolves(
        ctx,
        "../mod-self-slash/",
        Some("resolve/mod-self-slash/index.js"),
    );
    assert_resolves(
        ctx,
        "../mod-self-noslash",
        Some("resolve/mod-self-noslash/index.js"),
    );
    assert_resolves(
        ctx,
        "../mod-self-noslash/",
        Some("resolve/mod-self-noslash/index.js"),
    );
    assert_resolves(
        ctx,
        "../mod-outer/mod-parent-slash",
        Some("resolve/mod-outer/index.js"),
    );
    assert_resolves(
        ctx,
        "../mod-outer/mod-parent-slash/",
        Some("resolve/mod-outer/index.js"),
    );
    assert_resolves(
        ctx,
        "../mod-outer/mod-parent-noslash",
        Some("resolve/mod-outer/index.js"),
    );
    assert_resolves(
        ctx,
        "../mod-outer/mod-parent-noslash/",
        Some("resolve/mod-outer/index.js"),
    );
    assert_resolves(
        "resolve/mod-outer/mod-parent-slash/hypothetical.js",
        "..",
        Some("resolve/mod-outer/main.js"),
    );
    assert_resolves(
        "resolve/mod-outer/mod-parent-slash/hypothetical.js",
        "../",
        Some("resolve/mod-outer/main.js"),
    );
    assert_resolves(
        "resolve/mod-outer/mod-parent-noslash/hypothetical.js",
        "..",
        Some("resolve/mod-outer/main.js"),
    );
    assert_resolves(
        "resolve/mod-outer/mod-parent-noslash/hypothetical.js",
        "../",
        Some("resolve/mod-outer/main.js"),
    );

    assert_resolves(
        "resolve/dir-js/hypothetical.js",
        ".",
        Some("resolve/dir-js/index.js"),
    );
    assert_resolves(
        "resolve/dir-js/hypothetical.js",
        "./",
        Some("resolve/dir-js/index.js"),
    );
    assert_resolves(
        "resolve/dir-json/hypothetical.js",
        ".",
        Some("resolve/dir-json/index.json"),
    );
    assert_resolves(
        "resolve/dir-json/hypothetical.js",
        "./",
        Some("resolve/dir-json/index.json"),
    );
    assert_resolves(
        "resolve/dir-mjs/hypothetical.js",
        ".",
        Some("resolve/dir-mjs/index.mjs"),
    );
    assert_resolves(
        "resolve/dir-mjs/hypothetical.js",
        "./",
        Some("resolve/dir-mjs/index.mjs"),
    );

    assert_resolves(
        "resolve/mod-noext-bare/hypothetical.js",
        ".",
        Some("resolve/mod-noext-bare/main-noext"),
    );
    assert_resolves(
        "resolve/mod-noext-bare/hypothetical.js",
        "./",
        Some("resolve/mod-noext-bare/main-noext"),
    );
    assert_resolves(
        "resolve/mod-noext-rel/hypothetical.js",
        ".",
        Some("resolve/mod-noext-rel/main-noext"),
    );
    assert_resolves(
        "resolve/mod-noext-rel/hypothetical.js",
        "./",
        Some("resolve/mod-noext-rel/main-noext"),
    );

    assert_resolves(
        "resolve/mod-main-nesting-bare/hypothetical.js",
        ".",
        Some("resolve/mod-main-nesting-bare/subdir/index.js"),
    );
    assert_resolves(
        "resolve/mod-main-nesting-bare/hypothetical.js",
        "./",
        Some("resolve/mod-main-nesting-bare/subdir/index.js"),
    );
    assert_resolves(
        "resolve/mod-main-nesting-bare/subdir/hypothetical.js",
        ".",
        Some("resolve/mod-main-nesting-bare/subdir/inner-main.js"),
    );
    assert_resolves(
        "resolve/mod-main-nesting-bare/subdir/hypothetical.js",
        "./",
        Some("resolve/mod-main-nesting-bare/subdir/inner-main.js"),
    );
    assert_resolves(
        "resolve/mod-main-nesting-rel/hypothetical.js",
        ".",
        Some("resolve/mod-main-nesting-rel/subdir/index.js"),
    );
    assert_resolves(
        "resolve/mod-main-nesting-rel/hypothetical.js",
        "./",
        Some("resolve/mod-main-nesting-rel/subdir/index.js"),
    );
    assert_resolves(
        "resolve/mod-main-nesting-rel/subdir/hypothetical.js",
        "..",
        Some("resolve/mod-main-nesting-rel/subdir/index.js"),
    );
    assert_resolves(
        "resolve/mod-main-nesting-rel/subdir/hypothetical.js",
        "../",
        Some("resolve/mod-main-nesting-rel/subdir/index.js"),
    );
    assert_resolves(
        "resolve/mod-main-nesting-rel/subdir/hypothetical.js",
        ".",
        Some("resolve/mod-main-nesting-rel/subdir/inner-main.js"),
    );
    assert_resolves(
        "resolve/mod-main-nesting-rel/subdir/hypothetical.js",
        "./",
        Some("resolve/mod-main-nesting-rel/subdir/inner-main.js"),
    );

    assert_resolves(
        "resolve/mod-js-ext-bare/hypothetical.js",
        ".",
        Some("resolve/mod-js-ext-bare/main-js.js"),
    );
    assert_resolves(
        "resolve/mod-js-ext-bare/hypothetical.js",
        "./",
        Some("resolve/mod-js-ext-bare/main-js.js"),
    );
    assert_resolves(
        "resolve/mod-js-ext-rel/hypothetical.js",
        ".",
        Some("resolve/mod-js-ext-rel/main-js.js"),
    );
    assert_resolves(
        "resolve/mod-js-ext-rel/hypothetical.js",
        "./",
        Some("resolve/mod-js-ext-rel/main-js.js"),
    );
    assert_resolves(
        "resolve/mod-js-noext-bare/hypothetical.js",
        ".",
        Some("resolve/mod-js-noext-bare/main-js.js"),
    );
    assert_resolves(
        "resolve/mod-js-noext-bare/hypothetical.js",
        "./",
        Some("resolve/mod-js-noext-bare/main-js.js"),
    );
    assert_resolves(
        "resolve/mod-js-noext-rel/hypothetical.js",
        ".",
        Some("resolve/mod-js-noext-rel/main-js.js"),
    );
    assert_resolves(
        "resolve/mod-js-noext-rel/hypothetical.js",
        "./",
        Some("resolve/mod-js-noext-rel/main-js.js"),
    );
    assert_resolves(
        "resolve/mod-js-dir-bare/hypothetical.js",
        ".",
        Some("resolve/mod-js-dir-bare/main-js/index.js"),
    );
    assert_resolves(
        "resolve/mod-js-dir-bare/hypothetical.js",
        "./",
        Some("resolve/mod-js-dir-bare/main-js/index.js"),
    );
    assert_resolves(
        "resolve/mod-js-dir-bare/main-js/hypothetical.js",
        "..",
        Some("resolve/mod-js-dir-bare/main-js/index.js"),
    );
    assert_resolves(
        "resolve/mod-js-dir-bare/main-js/hypothetical.js",
        "../",
        Some("resolve/mod-js-dir-bare/main-js/index.js"),
    );
    assert_resolves(
        "resolve/mod-js-dir-rel/hypothetical.js",
        ".",
        Some("resolve/mod-js-dir-rel/main-js/index.js"),
    );
    assert_resolves(
        "resolve/mod-js-dir-rel/hypothetical.js",
        "./",
        Some("resolve/mod-js-dir-rel/main-js/index.js"),
    );
    assert_resolves(
        "resolve/mod-js-dir-rel/main-js/hypothetical.js",
        "..",
        Some("resolve/mod-js-dir-rel/main-js/index.js"),
    );
    assert_resolves(
        "resolve/mod-js-dir-rel/main-js/hypothetical.js",
        "../",
        Some("resolve/mod-js-dir-rel/main-js/index.js"),
    );

    assert_resolves(
        "resolve/mod-json-ext-bare/hypothetical.js",
        ".",
        Some("resolve/mod-json-ext-bare/main-json.json"),
    );
    assert_resolves(
        "resolve/mod-json-ext-bare/hypothetical.js",
        "./",
        Some("resolve/mod-json-ext-bare/main-json.json"),
    );
    assert_resolves(
        "resolve/mod-json-ext-rel/hypothetical.js",
        ".",
        Some("resolve/mod-json-ext-rel/main-json.json"),
    );
    assert_resolves(
        "resolve/mod-json-ext-rel/hypothetical.js",
        "./",
        Some("resolve/mod-json-ext-rel/main-json.json"),
    );
    assert_resolves(
        "resolve/mod-json-noext-bare/hypothetical.js",
        ".",
        Some("resolve/mod-json-noext-bare/main-json.json"),
    );
    assert_resolves(
        "resolve/mod-json-noext-bare/hypothetical.js",
        "./",
        Some("resolve/mod-json-noext-bare/main-json.json"),
    );
    assert_resolves(
        "resolve/mod-json-noext-rel/hypothetical.js",
        ".",
        Some("resolve/mod-json-noext-rel/main-json.json"),
    );
    assert_resolves(
        "resolve/mod-json-noext-rel/hypothetical.js",
        "./",
        Some("resolve/mod-json-noext-rel/main-json.json"),
    );
    assert_resolves(
        "resolve/mod-json-dir-bare/hypothetical.js",
        ".",
        Some("resolve/mod-json-dir-bare/main-json/index.json"),
    );
    assert_resolves(
        "resolve/mod-json-dir-bare/hypothetical.js",
        "./",
        Some("resolve/mod-json-dir-bare/main-json/index.json"),
    );
    assert_resolves(
        "resolve/mod-json-dir-rel/hypothetical.js",
        ".",
        Some("resolve/mod-json-dir-rel/main-json/index.json"),
    );
    assert_resolves(
        "resolve/mod-json-dir-rel/hypothetical.js",
        "./",
        Some("resolve/mod-json-dir-rel/main-json/index.json"),
    );

    assert_resolves(
        "resolve/mod-mjs-ext-bare/hypothetical.js",
        ".",
        Some("resolve/mod-mjs-ext-bare/main-mjs.mjs"),
    );
    assert_resolves(
        "resolve/mod-mjs-ext-bare/hypothetical.js",
        "./",
        Some("resolve/mod-mjs-ext-bare/main-mjs.mjs"),
    );
    assert_resolves(
        "resolve/mod-mjs-ext-rel/hypothetical.js",
        ".",
        Some("resolve/mod-mjs-ext-rel/main-mjs.mjs"),
    );
    assert_resolves(
        "resolve/mod-mjs-ext-rel/hypothetical.js",
        "./",
        Some("resolve/mod-mjs-ext-rel/main-mjs.mjs"),
    );
    assert_resolves(
        "resolve/mod-mjs-noext-bare/hypothetical.js",
        ".",
        Some("resolve/mod-mjs-noext-bare/main-mjs.mjs"),
    );
    assert_resolves(
        "resolve/mod-mjs-noext-bare/hypothetical.js",
        "./",
        Some("resolve/mod-mjs-noext-bare/main-mjs.mjs"),
    );
    assert_resolves(
        "resolve/mod-mjs-noext-rel/hypothetical.js",
        ".",
        Some("resolve/mod-mjs-noext-rel/main-mjs.mjs"),
    );
    assert_resolves(
        "resolve/mod-mjs-noext-rel/hypothetical.js",
        "./",
        Some("resolve/mod-mjs-noext-rel/main-mjs.mjs"),
    );
    assert_resolves(
        "resolve/mod-mjs-dir-bare/hypothetical.js",
        ".",
        Some("resolve/mod-mjs-dir-bare/main-mjs/index.mjs"),
    );
    assert_resolves(
        "resolve/mod-mjs-dir-bare/hypothetical.js",
        "./",
        Some("resolve/mod-mjs-dir-bare/main-mjs/index.mjs"),
    );
    assert_resolves(
        "resolve/mod-mjs-dir-bare/main-mjs/hypothetical.js",
        "..",
        Some("resolve/mod-mjs-dir-bare/main-mjs/index.mjs"),
    );
    assert_resolves(
        "resolve/mod-mjs-dir-bare/main-mjs/hypothetical.js",
        "../",
        Some("resolve/mod-mjs-dir-bare/main-mjs/index.mjs"),
    );
    assert_resolves(
        "resolve/mod-mjs-dir-rel/hypothetical.js",
        ".",
        Some("resolve/mod-mjs-dir-rel/main-mjs/index.mjs"),
    );
    assert_resolves(
        "resolve/mod-mjs-dir-rel/hypothetical.js",
        "./",
        Some("resolve/mod-mjs-dir-rel/main-mjs/index.mjs"),
    );
    assert_resolves(
        "resolve/mod-mjs-dir-rel/main-mjs/hypothetical.js",
        "..",
        Some("resolve/mod-mjs-dir-rel/main-mjs/index.mjs"),
    );
    assert_resolves(
        "resolve/mod-mjs-dir-rel/main-mjs/hypothetical.js",
        "../",
        Some("resolve/mod-mjs-dir-rel/main-mjs/index.mjs"),
    );

    assert_resolves(
        "resolve/mod-js-slash-bare/hypothetical.js",
        ".",
        Some("resolve/mod-js-slash-bare/main.js"),
    );
    assert_resolves(
        "resolve/mod-js-slash-bare/hypothetical.js",
        "./",
        Some("resolve/mod-js-slash-bare/main.js"),
    );
    assert_resolves(
        "resolve/mod-js-slash-bare/main/hypothetical.js",
        "..",
        Some("resolve/mod-js-slash-bare/main.js"),
    );
    assert_resolves(
        "resolve/mod-js-slash-bare/main/hypothetical.js",
        "../",
        Some("resolve/mod-js-slash-bare/main.js"),
    );
    assert_resolves(
        "resolve/mod-js-slash-rel/hypothetical.js",
        ".",
        Some("resolve/mod-js-slash-rel/main.js"),
    );
    assert_resolves(
        "resolve/mod-js-slash-rel/hypothetical.js",
        "./",
        Some("resolve/mod-js-slash-rel/main.js"),
    );
    assert_resolves(
        "resolve/mod-js-slash-rel/main/hypothetical.js",
        "..",
        Some("resolve/mod-js-slash-rel/main.js"),
    );
    assert_resolves(
        "resolve/mod-js-slash-rel/main/hypothetical.js",
        "../",
        Some("resolve/mod-js-slash-rel/main.js"),
    );

    assert_resolves(
        "resolve/file-and-dir/hypothetical.js",
        ".",
        Some("resolve/file-and-dir/index.js"),
    );
    assert_resolves(
        "resolve/file-and-dir/hypothetical.js",
        "./",
        Some("resolve/file-and-dir/index.js"),
    );
    assert_resolves(
        "resolve/file-and-dir/subdir/hypothetical.js",
        "..",
        Some("resolve/file-and-dir/index.js"),
    );
    assert_resolves(
        "resolve/file-and-dir/subdir/hypothetical.js",
        "../",
        Some("resolve/file-and-dir/index.js"),
    );
    assert_resolves(
        "resolve/file-and-mod/hypothetical.js",
        ".",
        Some("resolve/file-and-mod/main.js"),
    );
    assert_resolves(
        "resolve/file-and-mod/hypothetical.js",
        "./",
        Some("resolve/file-and-mod/main.js"),
    );
    assert_resolves(
        "resolve/file-and-mod/subdir/hypothetical.js",
        "..",
        Some("resolve/file-and-mod/main.js"),
    );
    assert_resolves(
        "resolve/file-and-mod/subdir/hypothetical.js",
        "../",
        Some("resolve/file-and-mod/main.js"),
    );

    let ctx = "resolve/hypothetical.js";
    assert_resolves(
        ctx,
        "./file-and-dir/submod",
        Some("resolve/file-and-dir.js"),
    );
    assert_resolves(
        ctx,
        "./file-and-dir/submod/",
        Some("resolve/file-and-dir.js"),
    );
    assert_resolves(
        ctx,
        "./file-and-mod/submod",
        Some("resolve/file-and-mod.js"),
    );
    assert_resolves(
        ctx,
        "./file-and-mod/submod/",
        Some("resolve/file-and-mod.js"),
    );

    // absolute paths

    let ctx = "resolve/subdir/hypothetical.js";
    let mut path = fixture_path();
    path.push("resolve/named-js");
    assert_resolves(ctx, path.to_str().unwrap(), Some("resolve/named-js.js"));

    // modules

    let ctx = "resolve/hypothetical.js";
    assert_resolves(
        ctx,
        "n-named-noext",
        Some("resolve/node_modules/n-named-noext"),
    );
    assert_resolves(
        ctx,
        "n-named-js.js",
        Some("resolve/node_modules/n-named-js.js"),
    );
    assert_resolves(
        ctx,
        "n-named-json.json",
        Some("resolve/node_modules/n-named-json.json"),
    );
    assert_resolves(
        ctx,
        "n-named-mjs.mjs",
        Some("resolve/node_modules/n-named-mjs.mjs"),
    );
    assert_resolves(
        ctx,
        "n-named-jsz.jsz",
        Some("resolve/node_modules/n-named-jsz.jsz"),
    );

    assert_resolves(
        ctx,
        "n-named-js",
        Some("resolve/node_modules/n-named-js.js"),
    );
    assert_resolves(
        ctx,
        "n-named-json",
        Some("resolve/node_modules/n-named-json.json"),
    );
    assert_resolves(
        ctx,
        "n-named-mjs",
        Some("resolve/node_modules/n-named-mjs.mjs"),
    );

    assert_resolves(
        ctx,
        "n-dir-js",
        Some("resolve/node_modules/n-dir-js/index.js"),
    );
    assert_resolves(
        ctx,
        "n-dir-js/index",
        Some("resolve/node_modules/n-dir-js/index.js"),
    );
    assert_resolves(
        ctx,
        "n-dir-json",
        Some("resolve/node_modules/n-dir-json/index.json"),
    );
    assert_resolves(
        ctx,
        "n-dir-json/index",
        Some("resolve/node_modules/n-dir-json/index.json"),
    );
    assert_resolves(
        ctx,
        "n-dir-mjs",
        Some("resolve/node_modules/n-dir-mjs/index.mjs"),
    );
    assert_resolves(
        ctx,
        "n-dir-mjs/index",
        Some("resolve/node_modules/n-dir-mjs/index.mjs"),
    );

    assert_resolves(
        ctx,
        "n-mod-noext-bare",
        Some("resolve/node_modules/n-mod-noext-bare/main-noext"),
    );
    assert_resolves(
        ctx,
        "n-mod-noext-rel",
        Some("resolve/node_modules/n-mod-noext-rel/main-noext"),
    );

    assert_resolves(
        ctx,
        "n-mod-main-nesting-bare",
        Some("resolve/node_modules/n-mod-main-nesting-bare/subdir/index.js"),
    );
    assert_resolves(
        ctx,
        "n-mod-main-nesting-bare/subdir",
        Some("resolve/node_modules/n-mod-main-nesting-bare/subdir/inner-main.js"),
    );
    assert_resolves(
        ctx,
        "n-mod-main-nesting-rel",
        Some("resolve/node_modules/n-mod-main-nesting-rel/subdir/index.js"),
    );
    assert_resolves(
        ctx,
        "n-mod-main-nesting-rel/subdir",
        Some("resolve/node_modules/n-mod-main-nesting-rel/subdir/inner-main.js"),
    );

    assert_resolves(
        ctx,
        "n-mod-js-ext-bare",
        Some("resolve/node_modules/n-mod-js-ext-bare/main-js.js"),
    );
    assert_resolves(
        ctx,
        "n-mod-js-ext-rel",
        Some("resolve/node_modules/n-mod-js-ext-rel/main-js.js"),
    );
    assert_resolves(
        ctx,
        "n-mod-js-noext-bare",
        Some("resolve/node_modules/n-mod-js-noext-bare/main-js.js"),
    );
    assert_resolves(
        ctx,
        "n-mod-js-noext-rel",
        Some("resolve/node_modules/n-mod-js-noext-rel/main-js.js"),
    );
    assert_resolves(
        ctx,
        "n-mod-js-dir-bare",
        Some("resolve/node_modules/n-mod-js-dir-bare/main-js/index.js"),
    );
    assert_resolves(
        ctx,
        "n-mod-js-dir-rel",
        Some("resolve/node_modules/n-mod-js-dir-rel/main-js/index.js"),
    );

    assert_resolves(
        ctx,
        "n-mod-json-ext-bare",
        Some("resolve/node_modules/n-mod-json-ext-bare/main-json.json"),
    );
    assert_resolves(
        ctx,
        "n-mod-json-ext-rel",
        Some("resolve/node_modules/n-mod-json-ext-rel/main-json.json"),
    );
    assert_resolves(
        ctx,
        "n-mod-json-noext-bare",
        Some("resolve/node_modules/n-mod-json-noext-bare/main-json.json"),
    );
    assert_resolves(
        ctx,
        "n-mod-json-noext-rel",
        Some("resolve/node_modules/n-mod-json-noext-rel/main-json.json"),
    );
    assert_resolves(
        ctx,
        "n-mod-json-dir-bare",
        Some("resolve/node_modules/n-mod-json-dir-bare/main-json/index.json"),
    );
    assert_resolves(
        ctx,
        "n-mod-json-dir-rel",
        Some("resolve/node_modules/n-mod-json-dir-rel/main-json/index.json"),
    );

    assert_resolves(
        ctx,
        "n-mod-mjs-ext-bare",
        Some("resolve/node_modules/n-mod-mjs-ext-bare/main-mjs.mjs"),
    );
    assert_resolves(
        ctx,
        "n-mod-mjs-ext-rel",
        Some("resolve/node_modules/n-mod-mjs-ext-rel/main-mjs.mjs"),
    );
    assert_resolves(
        ctx,
        "n-mod-mjs-noext-bare",
        Some("resolve/node_modules/n-mod-mjs-noext-bare/main-mjs.mjs"),
    );
    assert_resolves(
        ctx,
        "n-mod-mjs-noext-rel",
        Some("resolve/node_modules/n-mod-mjs-noext-rel/main-mjs.mjs"),
    );
    assert_resolves(
        ctx,
        "n-mod-mjs-dir-bare",
        Some("resolve/node_modules/n-mod-mjs-dir-bare/main-mjs/index.mjs"),
    );
    assert_resolves(
        ctx,
        "n-mod-mjs-dir-rel",
        Some("resolve/node_modules/n-mod-mjs-dir-rel/main-mjs/index.mjs"),
    );

    assert_resolves(
        ctx,
        "n-mod-js-slash-bare",
        Some("resolve/node_modules/n-mod-js-slash-bare/main.js"),
    );
    assert_resolves(
        ctx,
        "n-mod-js-slash-rel",
        Some("resolve/node_modules/n-mod-js-slash-rel/main.js"),
    );

    assert_resolves(ctx, "n-named-jsz", None);

    assert_resolves(
        ctx,
        "n-file-and-dir",
        Some("resolve/node_modules/n-file-and-dir.js"),
    );
    assert_resolves(
        ctx,
        "n-file-and-dir/",
        Some("resolve/node_modules/n-file-and-dir/index.js"),
    );
    assert_resolves(
        ctx,
        "n-file-and-mod",
        Some("resolve/node_modules/n-file-and-mod.js"),
    );
    assert_resolves(
        ctx,
        "n-file-and-mod/",
        Some("resolve/node_modules/n-file-and-mod/main.js"),
    );
    assert_resolves(
        ctx,
        "n-dir-js/",
        Some("resolve/node_modules/n-dir-js/index.js"),
    );
    assert_resolves(
        ctx,
        "n-mod-js-noext-rel/",
        Some("resolve/node_modules/n-mod-js-noext-rel/main-js.js"),
    );
    assert_resolves(ctx, "n-named-js.js/", None);
    assert_resolves(ctx, "n-named-js/", None);
    assert_resolves(ctx, "n-named-noext/", None);

    assert_resolves(ctx, "./n-named-noext", None);
    assert_resolves(ctx, "./n-named-js.js", None);
    assert_resolves(ctx, "./n-named-json.json", None);
    assert_resolves(ctx, "./n-named-mjs.mjs", None);
    assert_resolves(ctx, "./n-named-jsz.jsz", None);

    assert_resolves(ctx, "./n-named-js", None);
    assert_resolves(ctx, "./n-named-json", None);
    assert_resolves(ctx, "./n-named-mjs", None);

    assert_resolves(ctx, "./n-dir-js", None);
    assert_resolves(ctx, "./n-dir-js/index", None);
    assert_resolves(ctx, "./n-dir-json", None);
    assert_resolves(ctx, "./n-dir-json/index", None);
    assert_resolves(ctx, "./n-dir-mjs", None);
    assert_resolves(ctx, "./n-dir-mjs/index", None);

    assert_resolves(ctx, "./n-mod-noext-bare", None);
    assert_resolves(ctx, "./n-mod-noext-rel", None);

    assert_resolves(ctx, "./n-mod-main-nesting-bare", None);
    assert_resolves(ctx, "./n-mod-main-nesting-bare/subdir", None);
    assert_resolves(ctx, "./n-mod-main-nesting-rel", None);
    assert_resolves(ctx, "./n-mod-main-nesting-rel/subdir", None);

    assert_resolves(ctx, "./n-mod-js-ext-bare", None);
    assert_resolves(ctx, "./n-mod-js-ext-rel", None);
    assert_resolves(ctx, "./n-mod-js-noext-bare", None);
    assert_resolves(ctx, "./n-mod-js-noext-rel", None);
    assert_resolves(ctx, "./n-mod-js-dir-bare", None);
    assert_resolves(ctx, "./n-mod-js-dir-rel", None);

    assert_resolves(ctx, "./n-mod-json-ext-bare", None);
    assert_resolves(ctx, "./n-mod-json-ext-rel", None);
    assert_resolves(ctx, "./n-mod-json-noext-bare", None);
    assert_resolves(ctx, "./n-mod-json-noext-rel", None);
    assert_resolves(ctx, "./n-mod-json-dir-bare", None);
    assert_resolves(ctx, "./n-mod-json-dir-rel", None);

    assert_resolves(ctx, "./n-mod-mjs-ext-bare", None);
    assert_resolves(ctx, "./n-mod-mjs-ext-rel", None);
    assert_resolves(ctx, "./n-mod-mjs-noext-bare", None);
    assert_resolves(ctx, "./n-mod-mjs-noext-rel", None);
    assert_resolves(ctx, "./n-mod-mjs-dir-bare", None);
    assert_resolves(ctx, "./n-mod-mjs-dir-rel", None);

    assert_resolves(ctx, "./n-mod-js-slash-bare", None);
    assert_resolves(ctx, "./n-mod-js-slash-rel", None);

    assert_resolves(ctx, "./n-named-jsz", None);

    assert_resolves(ctx, "./n-file-and-dir", None);
    assert_resolves(ctx, "./n-file-and-dir/", None);
    assert_resolves(ctx, "./n-file-and-mod", None);
    assert_resolves(ctx, "./n-file-and-mod/", None);
    assert_resolves(ctx, "./n-dir-js/", None);
    assert_resolves(ctx, "./n-mod-js-noext-rel/", None);
    assert_resolves(ctx, "./n-named-js.js/", None);
    assert_resolves(ctx, "./n-named-js/", None);
    assert_resolves(ctx, "./n-named-noext/", None);

    assert_resolves(
        ctx,
        "shadowed",
        Some("resolve/node_modules/shadowed/index.js"),
    );

    assert_resolves(
        ctx,
        "@user/scoped",
        Some("resolve/node_modules/@user/scoped/index.js"),
    );
    assert_resolves(
        ctx,
        "@user/scoped/index",
        Some("resolve/node_modules/@user/scoped/index.js"),
    );
    assert_resolves(
        ctx,
        "@user/scoped/index.js",
        Some("resolve/node_modules/@user/scoped/index.js"),
    );

    assert_resolves(
        ctx,
        "shallow/s-named-noext",
        Some("resolve/node_modules/shallow/s-named-noext"),
    );
    assert_resolves(
        ctx,
        "shallow/s-named-js.js",
        Some("resolve/node_modules/shallow/s-named-js.js"),
    );
    assert_resolves(
        ctx,
        "shallow/s-named-json.json",
        Some("resolve/node_modules/shallow/s-named-json.json"),
    );
    assert_resolves(
        ctx,
        "shallow/s-named-mjs.mjs",
        Some("resolve/node_modules/shallow/s-named-mjs.mjs"),
    );
    assert_resolves(
        ctx,
        "shallow/s-named-jsz.jsz",
        Some("resolve/node_modules/shallow/s-named-jsz.jsz"),
    );

    assert_resolves(
        ctx,
        "shallow/s-named-js",
        Some("resolve/node_modules/shallow/s-named-js.js"),
    );
    assert_resolves(
        ctx,
        "shallow/s-named-json",
        Some("resolve/node_modules/shallow/s-named-json.json"),
    );
    assert_resolves(
        ctx,
        "shallow/s-named-mjs",
        Some("resolve/node_modules/shallow/s-named-mjs.mjs"),
    );

    assert_resolves(
        ctx,
        "shallow/s-dir-js",
        Some("resolve/node_modules/shallow/s-dir-js/index.js"),
    );
    assert_resolves(
        ctx,
        "shallow/s-dir-js/index",
        Some("resolve/node_modules/shallow/s-dir-js/index.js"),
    );
    assert_resolves(
        ctx,
        "shallow/s-dir-json",
        Some("resolve/node_modules/shallow/s-dir-json/index.json"),
    );
    assert_resolves(
        ctx,
        "shallow/s-dir-json/index",
        Some("resolve/node_modules/shallow/s-dir-json/index.json"),
    );
    assert_resolves(
        ctx,
        "shallow/s-dir-mjs",
        Some("resolve/node_modules/shallow/s-dir-mjs/index.mjs"),
    );
    assert_resolves(
        ctx,
        "shallow/s-dir-mjs/index",
        Some("resolve/node_modules/shallow/s-dir-mjs/index.mjs"),
    );

    assert_resolves(
        ctx,
        "shallow/s-mod-noext-bare",
        Some("resolve/node_modules/shallow/s-mod-noext-bare/main-noext"),
    );
    assert_resolves(
        ctx,
        "shallow/s-mod-noext-rel",
        Some("resolve/node_modules/shallow/s-mod-noext-rel/main-noext"),
    );

    assert_resolves(
        ctx,
        "shallow/s-mod-main-nesting-bare",
        Some("resolve/node_modules/shallow/s-mod-main-nesting-bare/subdir/index.js"),
    );
    assert_resolves(
        ctx,
        "shallow/s-mod-main-nesting-bare/subdir",
        Some("resolve/node_modules/shallow/s-mod-main-nesting-bare/subdir/inner-main.js"),
    );
    assert_resolves(
        ctx,
        "shallow/s-mod-main-nesting-rel",
        Some("resolve/node_modules/shallow/s-mod-main-nesting-rel/subdir/index.js"),
    );
    assert_resolves(
        ctx,
        "shallow/s-mod-main-nesting-rel/subdir",
        Some("resolve/node_modules/shallow/s-mod-main-nesting-rel/subdir/inner-main.js"),
    );

    assert_resolves(
        ctx,
        "shallow/s-mod-js-ext-bare",
        Some("resolve/node_modules/shallow/s-mod-js-ext-bare/main-js.js"),
    );
    assert_resolves(
        ctx,
        "shallow/s-mod-js-ext-rel",
        Some("resolve/node_modules/shallow/s-mod-js-ext-rel/main-js.js"),
    );
    assert_resolves(
        ctx,
        "shallow/s-mod-js-noext-bare",
        Some("resolve/node_modules/shallow/s-mod-js-noext-bare/main-js.js"),
    );
    assert_resolves(
        ctx,
        "shallow/s-mod-js-noext-rel",
        Some("resolve/node_modules/shallow/s-mod-js-noext-rel/main-js.js"),
    );
    assert_resolves(
        ctx,
        "shallow/s-mod-js-dir-bare",
        Some("resolve/node_modules/shallow/s-mod-js-dir-bare/main-js/index.js"),
    );
    assert_resolves(
        ctx,
        "shallow/s-mod-js-dir-rel",
        Some("resolve/node_modules/shallow/s-mod-js-dir-rel/main-js/index.js"),
    );

    assert_resolves(
        ctx,
        "shallow/s-mod-json-ext-bare",
        Some("resolve/node_modules/shallow/s-mod-json-ext-bare/main-json.json"),
    );
    assert_resolves(
        ctx,
        "shallow/s-mod-json-ext-rel",
        Some("resolve/node_modules/shallow/s-mod-json-ext-rel/main-json.json"),
    );
    assert_resolves(
        ctx,
        "shallow/s-mod-json-noext-bare",
        Some("resolve/node_modules/shallow/s-mod-json-noext-bare/main-json.json"),
    );
    assert_resolves(
        ctx,
        "shallow/s-mod-json-noext-rel",
        Some("resolve/node_modules/shallow/s-mod-json-noext-rel/main-json.json"),
    );
    assert_resolves(
        ctx,
        "shallow/s-mod-json-dir-bare",
        Some("resolve/node_modules/shallow/s-mod-json-dir-bare/main-json/index.json"),
    );
    assert_resolves(
        ctx,
        "shallow/s-mod-json-dir-rel",
        Some("resolve/node_modules/shallow/s-mod-json-dir-rel/main-json/index.json"),
    );

    assert_resolves(
        ctx,
        "shallow/s-mod-mjs-ext-bare",
        Some("resolve/node_modules/shallow/s-mod-mjs-ext-bare/main-mjs.mjs"),
    );
    assert_resolves(
        ctx,
        "shallow/s-mod-mjs-ext-rel",
        Some("resolve/node_modules/shallow/s-mod-mjs-ext-rel/main-mjs.mjs"),
    );
    assert_resolves(
        ctx,
        "shallow/s-mod-mjs-noext-bare",
        Some("resolve/node_modules/shallow/s-mod-mjs-noext-bare/main-mjs.mjs"),
    );
    assert_resolves(
        ctx,
        "shallow/s-mod-mjs-noext-rel",
        Some("resolve/node_modules/shallow/s-mod-mjs-noext-rel/main-mjs.mjs"),
    );
    assert_resolves(
        ctx,
        "shallow/s-mod-mjs-dir-bare",
        Some("resolve/node_modules/shallow/s-mod-mjs-dir-bare/main-mjs/index.mjs"),
    );
    assert_resolves(
        ctx,
        "shallow/s-mod-mjs-dir-rel",
        Some("resolve/node_modules/shallow/s-mod-mjs-dir-rel/main-mjs/index.mjs"),
    );

    assert_resolves(
        ctx,
        "shallow/s-mod-js-slash-bare",
        Some("resolve/node_modules/shallow/s-mod-js-slash-bare/main.js"),
    );
    assert_resolves(
        ctx,
        "shallow/s-mod-js-slash-rel",
        Some("resolve/node_modules/shallow/s-mod-js-slash-rel/main.js"),
    );

    assert_resolves(ctx, "shallow/s-named-jsz", None);

    assert_resolves(
        ctx,
        "shallow/s-file-and-dir",
        Some("resolve/node_modules/shallow/s-file-and-dir.js"),
    );
    assert_resolves(
        ctx,
        "shallow/s-file-and-dir/",
        Some("resolve/node_modules/shallow/s-file-and-dir/index.js"),
    );
    assert_resolves(
        ctx,
        "shallow/s-file-and-mod",
        Some("resolve/node_modules/shallow/s-file-and-mod.js"),
    );
    assert_resolves(
        ctx,
        "shallow/s-file-and-mod/",
        Some("resolve/node_modules/shallow/s-file-and-mod/main.js"),
    );
    assert_resolves(
        ctx,
        "shallow/s-dir-js/",
        Some("resolve/node_modules/shallow/s-dir-js/index.js"),
    );
    assert_resolves(
        ctx,
        "shallow/s-mod-js-noext-rel/",
        Some("resolve/node_modules/shallow/s-mod-js-noext-rel/main-js.js"),
    );
    assert_resolves(ctx, "shallow/s-named-js.js/", None);
    assert_resolves(ctx, "shallow/s-named-js/", None);
    assert_resolves(ctx, "shallow/s-named-noext/", None);

    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-named-noext",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-named-noext"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-named-js.js",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-named-js.js"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-named-json.json",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-named-json.json"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-named-mjs.mjs",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-named-mjs.mjs"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-named-jsz.jsz",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-named-jsz.jsz"),
    );

    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-named-js",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-named-js.js"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-named-json",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-named-json.json"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-named-mjs",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-named-mjs.mjs"),
    );

    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-dir-js",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-dir-js/index.js"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-dir-js/index",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-dir-js/index.js"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-dir-json",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-dir-json/index.json"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-dir-json/index",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-dir-json/index.json"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-dir-mjs",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-dir-mjs/index.mjs"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-dir-mjs/index",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-dir-mjs/index.mjs"),
    );

    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-mod-noext-bare",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-noext-bare/main-noext"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-mod-noext-rel",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-noext-rel/main-noext"),
    );

    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-mod-main-nesting-bare",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-main-nesting-bare/subdir/index.js"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-mod-main-nesting-bare/subdir",
        Some(
            "resolve/node_modules/deep/dir1/dir2/dir3/d-mod-main-nesting-bare/subdir/inner-main.js",
        ),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-mod-main-nesting-rel",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-main-nesting-rel/subdir/index.js"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-mod-main-nesting-rel/subdir",
        Some(
            "resolve/node_modules/deep/dir1/dir2/dir3/d-mod-main-nesting-rel/subdir/inner-main.js",
        ),
    );

    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-mod-js-ext-bare",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-js-ext-bare/main-js.js"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-mod-js-ext-rel",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-js-ext-rel/main-js.js"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-mod-js-noext-bare",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-js-noext-bare/main-js.js"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-mod-js-noext-rel",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-js-noext-rel/main-js.js"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-mod-js-dir-bare",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-js-dir-bare/main-js/index.js"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-mod-js-dir-rel",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-js-dir-rel/main-js/index.js"),
    );

    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-mod-json-ext-bare",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-json-ext-bare/main-json.json"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-mod-json-ext-rel",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-json-ext-rel/main-json.json"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-mod-json-noext-bare",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-json-noext-bare/main-json.json"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-mod-json-noext-rel",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-json-noext-rel/main-json.json"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-mod-json-dir-bare",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-json-dir-bare/main-json/index.json"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-mod-json-dir-rel",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-json-dir-rel/main-json/index.json"),
    );

    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-mod-mjs-ext-bare",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-mjs-ext-bare/main-mjs.mjs"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-mod-mjs-ext-rel",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-mjs-ext-rel/main-mjs.mjs"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-mod-mjs-noext-bare",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-mjs-noext-bare/main-mjs.mjs"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-mod-mjs-noext-rel",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-mjs-noext-rel/main-mjs.mjs"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-mod-mjs-dir-bare",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-mjs-dir-bare/main-mjs/index.mjs"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-mod-mjs-dir-rel",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-mjs-dir-rel/main-mjs/index.mjs"),
    );

    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-mod-js-slash-bare",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-js-slash-bare/main.js"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-mod-js-slash-rel",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-js-slash-rel/main.js"),
    );

    assert_resolves(ctx, "deep/dir1/dir2/dir3/d-named-jsz", None);

    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-file-and-dir",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-file-and-dir.js"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-file-and-dir/",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-file-and-dir/index.js"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-file-and-mod",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-file-and-mod.js"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-file-and-mod/",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-file-and-mod/main.js"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-dir-js/",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-dir-js/index.js"),
    );
    assert_resolves(
        ctx,
        "deep/dir1/dir2/dir3/d-mod-js-noext-rel/",
        Some("resolve/node_modules/deep/dir1/dir2/dir3/d-mod-js-noext-rel/main-js.js"),
    );
    assert_resolves(ctx, "deep/dir1/dir2/dir3/d-named-js.js/", None);
    assert_resolves(ctx, "deep/dir1/dir2/dir3/d-named-js/", None);
    assert_resolves(ctx, "deep/dir1/dir2/dir3/d-named-noext/", None);

    let ctx = "resolve/subdir/hypothetical.js";
    assert_resolves(
        ctx,
        "shadowed",
        Some("resolve/subdir/node_modules/shadowed/index.js"),
    );

    let ctx = "resolve/subdir/subdir2/hypothetical.js";
    assert_resolves(
        ctx,
        "shadowed",
        Some("resolve/subdir/subdir2/node_modules/shadowed/index.js"),
    );

    let ctx = "resolve/hypothetical.js";
    assert_resolves(ctx, "./dotfiles", None);
    assert_resolves(ctx, "./dotfiles/", None);

    assert_resolves(ctx, "./dotfiles/.thing", Some("resolve/dotfiles/.thing"));
    assert_resolves(
        ctx,
        "./dotfiles/.thing-js",
        Some("resolve/dotfiles/.thing-js.js"),
    );
    assert_resolves(
        ctx,
        "./dotfiles/.thing-js.js",
        Some("resolve/dotfiles/.thing-js.js"),
    );
    assert_resolves(
        ctx,
        "./dotfiles/.thing-json",
        Some("resolve/dotfiles/.thing-json.json"),
    );
    assert_resolves(
        ctx,
        "./dotfiles/.thing-json.json",
        Some("resolve/dotfiles/.thing-json.json"),
    );
    assert_resolves(
        ctx,
        "./dotfiles/.thing-mjs",
        Some("resolve/dotfiles/.thing-mjs.mjs"),
    );
    assert_resolves(
        ctx,
        "./dotfiles/.thing-mjs.mjs",
        Some("resolve/dotfiles/.thing-mjs.mjs"),
    );

    assert_resolves(ctx, "./dotfiles/.js", Some("resolve/dotfiles/.js"));
    assert_resolves(ctx, "./dotfiles/.json", Some("resolve/dotfiles/.json"));
    assert_resolves(ctx, "./dotfiles/.mjs", Some("resolve/dotfiles/.mjs"));

    assert_resolves(
        ctx,
        "./dotfiles/mod-noext",
        Some("resolve/dotfiles/mod-noext/.thing"),
    );
    assert_resolves(
        ctx,
        "./dotfiles/mod-js",
        Some("resolve/dotfiles/mod-js/.thing-js.js"),
    );
    assert_resolves(
        ctx,
        "./dotfiles/mod-json",
        Some("resolve/dotfiles/mod-json/.thing-json.json"),
    );
    assert_resolves(
        ctx,
        "./dotfiles/mod-mjs",
        Some("resolve/dotfiles/mod-mjs/.thing-mjs.mjs"),
    );

    let ctx = "resolve-order/hypothetical.js";
    assert_resolves(ctx, "./1-file", Some("resolve-order/1-file"));
    assert_resolves(ctx, "./2-file", Some("resolve-order/2-file.js"));
    assert_resolves(ctx, "./3-file", Some("resolve-order/3-file.json"));
    assert_resolves(ctx, "./1-dir", Some("resolve-order/1-dir.js"));
    assert_resolves(ctx, "./2-dir", Some("resolve-order/2-dir.json"));
    assert_resolves(ctx, "./3-dir", Some("resolve-order/3-dir/index.js"));
    assert_resolves(ctx, "./4-dir", Some("resolve-order/4-dir/index.json"));
    assert_resolves(ctx, "./1-dir/", Some("resolve-order/1-dir/index.js"));
    assert_resolves(ctx, "./2-dir/", Some("resolve-order/2-dir/index.js"));
    assert_resolves(ctx, "./3-dir/", Some("resolve-order/3-dir/index.js"));
    assert_resolves(ctx, "./4-dir/", Some("resolve-order/4-dir/index.json"));
}

fn test_resolve_unicode_with<F>(mut assert_resolves: F)
where
    F: FnMut(&str, &str, Option<&str>),
{
    let ctx = "resolve/hypothetical.js";
    assert_resolves(ctx, "./unicode/𝌆", Some("resolve/unicode/𝌆.js"));
    assert_resolves(ctx, "./unicode/𝌆.js", Some("resolve/unicode/𝌆.js"));
}

#[test]
fn test_resolve() {
    test_resolve_with(assert_resolves);
}

#[test]
fn test_resolve_unicode() {
    if cfg!(windows) {
        return;
    }
    test_resolve_unicode_with(assert_resolves);
}

#[test]
fn test_resolve_bower() {
    if cfg!(windows) {
        return;
    }
    test_resolve_bower_with(assert_resolves_bower);
}

fn test_resolve_bower_with<F>(mut assert_resolves: F)
where
    F: FnMut(&str, &str, Option<&str>),
{
    let ctx = "bower/hypothetical.js";
    assert_resolves(
        ctx,
        "single-js-array",
        Some("bower/bower_components/single-js-array/main.js"),
    );
    assert_resolves(
        ctx,
        "single-js-array/main.js",
        Some("bower/bower_components/single-js-array/main.js"),
    );
    assert_resolves(
        ctx,
        "single-js-entry",
        Some("bower/bower_components/single-js-entry/main.js"),
    );
    assert_resolves(
        ctx,
        "single-js-entry/main.js",
        Some("bower/bower_components/single-js-entry/main.js"),
    );
    assert_resolves(
        ctx,
        "js-and-sass-entries/main.js",
        Some("bower/bower_components/js-and-sass-entries/main.js"),
    );
    assert_resolves(
        ctx,
        "sass-and-js-entries/main.js",
        Some("bower/bower_components/sass-and-js-entries/main.js"),
    );
    assert_resolves(
        ctx,
        "sass-and-js-entries",
        Some("bower/bower_components/sass-and-js-entries/main.js"),
    );
    assert_resolves(
        ctx,
        "js-and-sass-entries",
        Some("bower/bower_components/js-and-sass-entries/main.js"),
    );

    assert_resolves(
        "bower/bower_components/dependency-with-bower-dot-json/hypothetical.js",
        "dependency",
        Some("bower/bower_components/dependency-with-bower-dot-json/bower_components/dependency/main.js"),
    );

    assert_resolves(
        "bower/bower_components/dependency-with-dot-bower-dot-json/hypothetical.js",
        "dependency",
        Some("bower/bower_components/dependency-with-dot-bower-dot-json/bower_components/dependency/main.js"),
    );
}

fn test_browser_with<F>(mut assert_resolves: F)
where
    F: FnMut(&str, &str, Option<&str>),
{
    let ctx = "browser/hypothetical.js";
    assert_resolves(
        ctx,
        "./alternate-main-rel",
        Some("browser/alternate-main-rel/main-default.js"),
    );
    assert_resolves(
        ctx,
        "./alternate-main-rel/main-default.js",
        Some("browser/alternate-main-rel/main-default.js"),
    );
    assert_resolves(
        ctx,
        "./alternate-main-bare",
        Some("browser/alternate-main-bare/main-default.js"),
    );
    assert_resolves(
        ctx,
        "./alternate-main-bare/main-default.js",
        Some("browser/alternate-main-bare/main-default.js"),
    );
    assert_resolves(
        ctx,
        "./alternate-main-rel",
        Some("browser/alternate-main-rel/main-browser.js"),
    );
    assert_resolves(
        ctx,
        "./alternate-main-rel/main-default.js",
        Some("browser/alternate-main-rel/main-browser.js"),
    );
    assert_resolves(
        ctx,
        "./alternate-main-bare",
        Some("browser/alternate-main-bare/main-browser.js"),
    );
    assert_resolves(
        ctx,
        "./alternate-main-bare/main-default.js",
        Some("browser/alternate-main-bare/main-browser.js"),
    );
}

#[test]
fn test_resolve_browser() {
    test_browser_with(assert_resolves);
}

#[test]
fn test_external() {
    fn fixture_path() -> PathBuf {
        let mut path = std::env::current_dir().unwrap();
        path.push("fixtures");
        path
    }
    fn assert_resolves(context: &str, from: &str, to: Resolved, options: &InputOptions) {
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

        let resolver = Resolver::new(options.clone());
        // resolves with an empty cache...
        assert_eq!(resolver.resolve(&context_path, from).unwrap(), to);
        // ...and with everything cached
        assert_eq!(resolver.resolve(&context_path, from).unwrap(), to);
    }

    let ext = InputOptions {
        package_manager: PackageManager::Npm,
        external: vec!["external".to_owned(), "external-only-module".to_owned()]
            .into_iter()
            .collect(),
    };
    let non = InputOptions {
        package_manager: PackageManager::Npm,
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
    assert_resolves(
        ctx,
        "./external",
        Resolved::Normal(PathBuf::from("resolve/external.js")),
        &ext,
    );

    assert_resolves(
        ctx,
        "./external",
        Resolved::Normal(PathBuf::from("resolve/external.js")),
        &non,
    );
    assert_resolves(
        ctx,
        "external",
        Resolved::Normal(PathBuf::from("resolve/node_modules/external/index.js")),
        &non,
    );
    assert_resolves(
        ctx,
        "external/",
        Resolved::Normal(PathBuf::from("resolve/node_modules/external/index.js")),
        &non,
    );
    assert_resolves(
        ctx,
        "external/file.js",
        Resolved::Normal(PathBuf::from("resolve/node_modules/external/file.js")),
        &non,
    );
    assert_resolves(
        ctx,
        "external/file",
        Resolved::Normal(PathBuf::from("resolve/node_modules/external/file.js")),
        &non,
    );
    assert_resolves(
        ctx,
        "external/subdir",
        Resolved::Normal(PathBuf::from(
            "resolve/node_modules/external/subdir/index.js",
        )),
        &non,
    );
    assert_resolves(
        ctx,
        "external/subdir/index",
        Resolved::Normal(PathBuf::from(
            "resolve/node_modules/external/subdir/index.js",
        )),
        &non,
    );
    assert_resolves(
        ctx,
        "external/subdir/index.js",
        Resolved::Normal(PathBuf::from(
            "resolve/node_modules/external/subdir/index.js",
        )),
        &non,
    );
}

#[test]
fn test_resolve_consistency() {
    fn fixture_path() -> PathBuf {
        let mut path = std::env::current_dir().unwrap();
        path.push("fixtures");
        path
    }

    if cfg!(windows) {
        return;
    }
    // meta-test: ensure test_resolve matches node behavior

    type Cases = FnvHashSet<(String, Option<String>)>;
    type CaseMap = FnvHashMap<String, Cases>;

    let mut assertions = FnvHashMap::default();

    {
        let mut append = |ctx: &str, from: &str, to: Option<&str>| {
            assertions
                .entry(ctx.to_owned())
                .or_insert_with(FnvHashSet::default)
                .insert((from.to_owned(), to.map(ToOwned::to_owned)));
        };

        // TODO figure out which of test_resolve_with tests hangs
        // test_resolve_with(&mut append);
        //
        test_resolve_unicode_with(&mut append);
        test_browser_with(&mut append);
        test_resolve_bower_with(&mut append);
    }

    fn make_source(base: &Path, cases: &Cases) -> Vec<u8> {
        let mut b = indoc!(
            br#"
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
        "#
        )
        .to_vec();
        for (from, to) in cases {
            let from_path = Path::new(from);
            let from = if from_path.is_absolute() {
                let suffix = from_path
                    .strip_prefix(fixture_path())
                    .expect("absolute path outside of fixtures");
                serde_json::to_string(&base.join(suffix))
            } else {
                serde_json::to_string(from)
            }
            .unwrap();
            if let Some(to) = to {
                let mut to_path = base.to_owned();
                to_path.append_resolving(to);
                let to = serde_json::to_string(to_path.to_str().unwrap()).unwrap();
                writeln!(b, "y({from}, {to})", from = from, to = to).unwrap();
            } else {
                writeln!(b, "n({from})", from = from).unwrap();
            }
        }
        // io::stdout().write_all(&b).unwrap();
        b
    }
    fn test_file(base: &Path, ctx: &str, cases: &Cases) {
        let mut ctx_dir = base.to_owned();
        ctx_dir.append_resolving(ctx);
        ctx_dir.pop();
        let ext = ".js";

        let mut file = tempfile::Builder::new()
            .suffix(ext)
            .tempfile_in(&ctx_dir)
            .unwrap();
        file.as_file_mut()
            .write_all(&make_source(base, cases))
            .unwrap();

        let path = file.path().to_str().unwrap();
        let output;
        let to_file = tempfile::Builder::new()
            .suffix(ext)
            .tempfile_in(&ctx_dir)
            .unwrap();
        let to_path = to_file.path().to_str().unwrap();

        // let mut browserify_path = fixture_path();
        // browserify_path.push("tools/node_modules/.bin/browserify");
        let browserify_path = base.join("tools/node_modules/.bin/browserify");
        dbg!(browserify_path.exists());
        let ok = process::Command::new(browserify_path)
            .stdout(process::Stdio::piped())
            .args(&[&path, &to_path])
            .status()
            .expect("failed to run browserify")
            .success();
        if !ok {
            panic!("browserify failed");
        }

        output = process::Command::new("node")
            .args(&[&to_path])
            .output()
            .expect("failed to run node");

        if !output.status.success() {
            io::stderr().write(&output.stderr).unwrap();
            panic!("tests are inconsistent with node/browserify");
        }
    }

    fn test_file_map(base: &Path, map: &CaseMap) {
        for (ctx, cases) in map.into_iter() {
            test_file(base, ctx, cases)
        }
    }

    let base_dir = tempfile::tempdir().unwrap();
    let fixture_dir = fixture_path();
    for entry in WalkDir::new(&fixture_dir)
        .into_iter()
        .filter_map(Result::ok)
    {
        let local_path = entry.path().strip_prefix(&fixture_dir).unwrap();
        if local_path.components().next().is_none() {
            continue;
        }

        let new_path = base_dir.path().join(local_path);
        // println!("{} {}", entry.path().display(), new_path.display());
        if !local_path.starts_with("tools/node_modules") {
            if entry.file_type().is_dir() {
                fs::create_dir(new_path).unwrap();
            } else {
                fs::copy(entry.path(), new_path).unwrap();
            }
        }
    }
    crate::npm_install(&base_dir.path().join("tools"));
    test_file_map(base_dir.path(), &assertions);
}
