# Scrumple

A fast (and scrappy) JavaScript bundler for developing Origami components.

## Why?

### Fast! :crab:

During development we want compilation to be as fast as possible.

### Origami specific

Scrumple is based on a now-deleted tool called Pax, but we have added support for `bower_components` and optimised the code for Origami components. For example, it uses the `browser` field by default when building an npm component.

### Scrappy

This is a developer tool, not production software. It's designed to give the developer super-fast feedback when building components, not for building a production application.

## Usage

```
Usage: scrumple [options] <input> [output]
       scrumple [-h | --help | -v | --version]

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

    -x, --external <module1,module2,...>
        Don't resolve or include modules named <module1>, <module2>, etc.;
        leave them as require('<module>') references in the bundle. Specifying
        a path instead of a module name does nothing.

    --external-core
        Ignore references to node.js core modules like 'events' and leave them
        as require('<module>') references in the bundle.

    -b, --for-bower
        Use bower.json instead of package.json

    -h, --help
        Print this message.

    -v, --version
        Print version information.
```
