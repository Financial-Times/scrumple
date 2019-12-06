use fnv::FnvHashSet;
use std::borrow::Cow;
use std::fmt::Write;

use esparse::eat;
use esparse::lex::{self, Tt};
use esparse::skip::{self, Prec};
use matches::matches;

pub mod error;
pub mod export;
pub mod import;

use self::error::*;
use self::export::*;
use self::import::*;

// Define our own error and result for some reason
// TODO figure out if we can use the regular one so we can use `?` instead of
// `.unwrap`. (or fix the traits??)
pub type Result<T> = ::std::result::Result<T, Error>;

macro_rules! expected {
    ($lex:expr, $msg:expr) => {{
        return Err(Error {
            kind: ErrorKind::Expected($msg),
            span: $lex.recover_span($lex.here().span).with_owned(),
        });
    }};
}

struct ImportBindingNames {
    names: Vec<String>,
    calls: Vec<String>,
}

impl ImportBindingNames {
    fn new() -> ImportBindingNames {
        ImportBindingNames {
            names: Vec::default(),
            calls: Vec::default(),
        }
    }

    fn add(&mut self, binding: String) {
        let call = format!("imports[\"{}\"]", &binding);
        self.calls.push(call);
        self.names.push(binding);
    }

    fn get_signature_params(&self) -> String {
        self.names.join(", ")
    }

    fn get_call_params(&self) -> String {
        self.calls.join(", ")
    }
}

#[derive(Debug)]
pub struct CjsModule<'s> {
    pub source_prefix: String,
    pub source: String,
    pub source_suffix: String,
    pub deps: FnvHashSet<Cow<'s, str>>,
}

pub fn module_to_cjs<'f, 's>(
    lex: &mut lex::Lexer<'f, 's>,
    allow_require: bool,
) -> Result<CjsModule<'s>> {
    let mut source = String::new();
    let mut deps = FnvHashSet::default();
    let mut imports = Vec::new();
    let mut exports = Vec::new();

    // TODO source map lines won't match up when module string literal contains
    // newlines
    // We only care about code about the code enough to collect imports, exports
    // and optionally cjs requires
    loop {
        eat!(lex => tok { source.push_str(tok.ws_before) },
            Tt::Export => {
                let export = parse_export(lex, &mut source)?;
                exports.push(export);
            },
            Tt::Import => {
                match parse_import(lex, &mut source)? {
                    ParsedImport::Import(import) => {
                        imports.push(import);
                    }
                    ParsedImport::ImportMeta => {}
                }
            },
            Tt::Id("require") if allow_require => {
                let start_pos = tok.span.start;
                // TODO wrap the `eat!` macro so we don't need all these `_ => {}` ?
                eat!(lex,
                    Tt::Lparen => eat!(lex,
                        Tt::StrLitSgl(dep_source) |
                        Tt::StrLitDbl(dep_source) => eat!(lex,
                            Tt::Rparen => {
                                deps.insert(match lex::str_lit_value(dep_source) {
                                    Ok(dep) => dep,
                                    Err(error) => return Err(Error {
                                        kind: ErrorKind::ParseStrLitError(error),
                                        span: lex.recover_span(tok.span).with_owned(),
                                    }),
                                });
                            },
                            _ => {},
                        ),
                        _ => {},
                    ),
                    _ => {},
                );

                let here = lex.here();
                let end_pos = here.span.start - here.ws_before.len();
                source.push_str(&lex.input()[start_pos..end_pos]);
            },
            Tt::Eof => break,
            _ => {
                let tok = lex.advance();
                write!(source, "{}{}", tok.ws_before, tok.tt).unwrap();
            },
        );
    }

    // our definition of an es6 module is:
    // if cjs is disabled, then it is a module
    // if it contains any exports or imports it's a module
    // TODO investigate if `export {}` counts as a module, and if that matters
    // either way
    let is_module = !allow_require || !exports.is_empty() || !imports.is_empty();
    let mut source_prefix = String::new();

    if is_module {
        write!(
            source_prefix,
            "Object.defineProperty(exports, '__esModule', {{value: true}})\n"
        )
        .unwrap();
    }

    // Create an item to collect the import names for the main function signature
    let mut import_bindings = ImportBindingNames::new();

    if !imports.is_empty() {
        write!(source_prefix, "var imports = (function() {{").unwrap();
        for (module_id, import) in imports.iter().enumerate() {
            write!(
                source_prefix,
                "\n  const __module{} = require._esModule({})",
                module_id, import.module_source
            )
            .unwrap();
        }
        write!(source_prefix, "\n  return Object.create(null, {{\n    [Symbol.toStringTag]: {{value: 'ModuleImports'}},").unwrap();
        for (module_id, import) in imports.iter().enumerate() {
            if let Some(default_binding) = import.default_bind {
                write!(
                    source_prefix,
                    "\n    {}: {{get() {{return __module{}.default}}, enumerable: true}},",
                    default_binding, module_id,
                )
                .unwrap();
                import_bindings.add(default_binding.to_owned());
            }
            match import.binds {
                Bindings::None => {}
                Bindings::NameSpace(wildcard_binding) => {
                    write!(
                        source_prefix,
                        "\n    {}: {{value: __module{}, enumerable: true}},",
                        wildcard_binding, module_id,
                    )
                    .unwrap();
                    import_bindings.add(wildcard_binding.to_owned());
                }
                Bindings::Named(ref specs) => {
                    for spec in specs {
                        let named_import_binding = spec.bind;
                        write!(
                            source_prefix,
                            "\n    {}: {{get() {{return __module{}.{}}}, enumerable: true}},",
                            named_import_binding, module_id, spec.name,
                        )
                        .unwrap();
                        import_bindings.add(named_import_binding.to_owned());
                    }
                }
            }
        }
        // TODO make this not nasty. shouldn't closing braces be the automatic
        // result of leaving the scope where the brace was opened?
        // TODO do we need a macro for inserting javascript?
        write!(source_prefix, "\n  }})\n}}()); ").unwrap();
    }

    // TODO use `()` instead of the bitwise not, which is just being fancy for
    // the sake of it
    write!(
        source_prefix,
        "; ~function({}) {{",
        import_bindings.get_signature_params()
    )
    .unwrap();

    if is_module {
        write!(source_prefix, "\n'use strict';\n").unwrap();
    }

    if !exports.is_empty() {
        let mut inner = String::new();
        let mut had_binds = false;

        write!(inner, "Object.defineProperties(exports, {{").unwrap();
        for (export_id, export) in exports.iter().enumerate() {
            match *export {
                Export::Default(default_binding) => {
                    write!(
                        inner,
                        "\n  default: {{get() {{return {}}}, enumerable: true}},",
                        default_binding,
                    )
                    .unwrap();
                }
                Export::Named(ref specs) => {
                    for spec in specs {
                        write!(
                            inner,
                            "\n  {}: {{get() {{return {}}}, enumerable: true}},",
                            spec.name, spec.bind,
                        )
                        .unwrap();
                    }
                }
                Export::AllFrom(name_source, _) => {
                    write!(
                        source_prefix,
                        "Object.defineProperties(exports, Object.getOwnPropertyDescriptors(require._esModule({})))\n",
                        name_source,
                    ).unwrap();
                }
                Export::NamedFrom(ref specs, name_source, _) => {
                    if !had_binds {
                        // TODO use `()` instead of the bitwise not, which is
                        // just being fancy for the sake of it
                        write!(source_prefix, "~function() {{\n").unwrap();
                        had_binds = true;
                    }
                    write!(
                        source_prefix,
                        "const __reexport{} = require._esModule({})\n",
                        export_id, name_source
                    )
                    .unwrap();

                    for spec in specs {
                        write!(
                            inner,
                            "\n  {}: {{get() {{return __reexport{}.{}}}, enumerable: true}},",
                            spec.name, export_id, spec.bind,
                        )
                        .unwrap();
                    }
                }
            }
        }

        write!(source_prefix, "{}\n}});\n", inner).unwrap();

        if had_binds {
            write!(source_prefix, "}}();\n").unwrap();
        }
    }

    for import in imports {
        deps.insert(import.module);
    }

    for export in exports {
        match export {
            Export::Default(_) => {}
            Export::Named(_) => {}
            Export::AllFrom(_, name) | Export::NamedFrom(_, _, name) => {
                deps.insert(name);
            }
        }
    }

    Ok(CjsModule {
        source_prefix,
        source,
        source_suffix: format!("}}({})", import_bindings.get_call_params()),
        deps,
    })
}

// TODO investigate this for full esm compat
// TODO address other TODO comments and code that's been commented out
#[inline(always)]
fn parse_export<'f, 's>(lex: &mut lex::Lexer<'f, 's>, source: &mut String) -> Result<Export<'s>> {
    eat!(lex => tok { source.push_str(tok.ws_before) },
        Tt::Default => {
            eat!(lex => tok,
                Tt::Class => {
                    write!(source, "{}{}", tok.ws_before, tok.tt).unwrap();
                    let name = eat!(lex => tok { write!(source, "{}{}", tok.ws_before, tok.tt).unwrap(); },
                        Tt::Id(name) => name,
                        _ => expected!(lex, "class name"),
                    );
                    Ok(Export::Default(name))
                },
                Tt::Function => {
                    write!(source, "{}{}", tok.ws_before, tok.tt).unwrap();
                    eat!(lex => tok { write!(source, "{}{}", tok.ws_before, tok.tt).unwrap(); },
                        Tt::Star => {},
                        _ => {},
                    );
                    let name = eat!(lex => tok { write!(source, "{}{}", tok.ws_before, tok.tt).unwrap(); },
                        Tt::Id(name) => name,
                        _ => expected!(lex, "function name"),
                    );
                    Ok(Export::Default(name))
                },
                Tt::Id("async") => {
                    if !lex.here().nl_before && matches!(lex.here().tt, Tt::Function) {
                        let tok2 = lex.advance();
                        write!(source, "{}{}", tok.ws_before, tok.tt).unwrap();
                        write!(source, "{}{}", tok2.ws_before, tok2.tt).unwrap();
                        eat!(lex => tok { write!(source, "{}{}", tok.ws_before, tok.tt).unwrap(); },
                            Tt::Star => {},
                            _ => {},
                        );
                        let name = eat!(lex => tok { write!(source, "{}{}", tok.ws_before, tok.tt).unwrap(); },
                            Tt::Id(name) => name,
                            _ => expected!(lex, "function name"),
                        );
                        Ok(Export::Default(name))
                    } else {
                        write!(source, "const __default = {}{}", tok.ws_before, tok.tt).unwrap();
                        // skip::expr(lex, Prec::NoComma)?;
                        Ok(Export::Default("__default"))
                    }
                },
                _ => {
                    source.push_str("const __default = ");
                    // skip::expr(lex, Prec::NoComma)?;
                    Ok(Export::Default("__default"))
                },
            )
        },
        Tt::Star => eat!(lex => tok { source.push_str(tok.ws_before) },
            Tt::Id("from") => eat!(lex => tok { source.push_str(tok.ws_before) },
                Tt::StrLitSgl(module_source) |
                Tt::StrLitDbl(module_source) => {
                    Ok(Export::AllFrom(module_source, match lex::str_lit_value(module_source) {
                        Ok(module) => module,
                        Err(error) => return Err(Error {
                            kind: ErrorKind::ParseStrLitError(error),
                            span: lex.recover_span(tok.span).with_owned(),
                        }),
                    }))
                },
                _ => expected!(lex, "module name (string literal)"),
            ),
            _ => expected!(lex, "keyword 'from'"),
        ),
        Tt::Lbrace => {
            let mut exports = Vec::new();
            loop {
                eat!(lex => tok { source.push_str(tok.ws_before) },
                    Tt::Id(_) |
                    Tt::Default => {
                        let bind = tok.tt.as_str();
                        eat!(lex => tok { source.push_str(tok.ws_before) },
                            Tt::Id("as") => eat!(lex => tok { source.push_str(tok.ws_before) },
                                Tt::Id(_) |
                                Tt::Default => {
                                    let name = tok.tt.as_str();
                                    exports.push(ExportSpec::new(bind, name));
                                    eat!(lex => tok { source.push_str(tok.ws_before) },
                                        Tt::Rbrace => break,
                                        Tt::Comma => {},
                                        _ => expected!(lex, "',' or '}'"),
                                    );
                                },
                                _ => expected!(lex, "export name after keyword 'as'"),
                            ),
                            Tt::Rbrace => {
                                exports.push(ExportSpec::same(bind));
                                break
                            },
                            Tt::Comma => {
                                exports.push(ExportSpec::same(bind));
                            },
                            _ => expected!(lex, "',' or '}' or keyword 'as'"),
                        )
                    },
                    Tt::Rbrace => break,
                    _ => expected!(lex, "binding name or '}'"),
                );
            }
            eat!(lex => tok { source.push_str(tok.ws_before) },
                Tt::Id("from") => eat!(lex => tok { source.push_str(tok.ws_before) },
                    Tt::StrLitSgl(module_source) |
                    Tt::StrLitDbl(module_source) => {
                        Ok(Export::NamedFrom(exports, module_source, match lex::str_lit_value(module_source) {
                            Ok(module) => module,
                            Err(error) => return Err(Error {
                                kind: ErrorKind::ParseStrLitError(error),
                                span: lex.recover_span(tok.span).with_owned(),
                            }),
                        }))
                    },
                    _ => expected!(lex, "module name (string literal)"),
                ),
                _ => {
                    Ok(Export::Named(exports))
                },
            )
        },
        Tt::Var |
        Tt::Const |
        Tt::Id("let") => {
            let start_pos = tok.span.start;
            let mut exports = Vec::new();
            loop {
                eat!(lex,
                    Tt::Id(name) => {
                        exports.push(ExportSpec::same(name));
                        eat!(lex,
                            Tt::Eq => {
                                skip::expr(lex, Prec::NoComma)?;
                                eat!(lex,
                                    Tt::Comma => continue,
                                    _ => break,
                                )
                            },
                            Tt::Comma => continue,
                            _ => break,
                        );
                    },
                    // TODO Tt::Lbrace =>
                    // TODO Tt::Lbracket =>
                    _ => expected!(lex, "binding name"),
                );
            }

            let here = lex.here();
            let end_pos = here.span.start - here.ws_before.len();
            source.push_str(&lex.input()[start_pos..end_pos]);

            Ok(Export::Named(exports))
        },
        Tt::Function => {
            let start_pos = tok.span.start;

            eat!(lex,
                Tt::Star => {},
                _ => {},
            );
            let name = eat!(lex,
                Tt::Id(name) => name,
                _ => expected!(lex, "function name"),
            );
            // eat!(lex,
            //     Tt::Lparen => skip::balanced_parens(lex, 1)?,
            //     _ => expected!(lex, "formal parameter list"),
            // );
            // eat!(lex,
            //     Tt::Lbrace => skip::balanced_braces(lex, 1)?,
            //     _ => expected!(lex, "function body"),
            // );

            let here = lex.here();
            let end_pos = here.span.start - here.ws_before.len();
            source.push_str(&lex.input()[start_pos..end_pos]);

            Ok(Export::Named(vec![ExportSpec::same(name)]))
        },
        Tt::Class => {
            let start_pos = tok.span.start;

            let name = eat!(lex,
                Tt::Id(name) => name,
                _ => expected!(lex, "class name"),
            );
            // eat!(lex,
            //     Tt::Extends => skip::expr(lex, Prec::NoComma)?,
            //     _ => {},
            // );
            // eat!(lex,
            //     Tt::Lbrace => skip::balanced_braces(lex, 1)?,
            //     _ => expected!(lex, "class body"),
            // );

            let here = lex.here();
            let end_pos = here.span.start - here.ws_before.len();
            source.push_str(&lex.input()[start_pos..end_pos]);

            Ok(Export::Named(vec![ExportSpec::same(name)]))
        },
        Tt::Id("async") => {
            let start_pos = tok.span.start;

            eat!(lex => tok,
                Tt::Function => {
                    if tok.nl_before {
                        expected!(lex, "no line terminator between 'function' and 'async'")
                    }
                },
                _ => expected!(lex, "'function' following 'async'"),
            );
            eat!(lex,
                Tt::Star => {},
                _ => {},
            );
            let name = eat!(lex,
                Tt::Id(name) => name,
                _ => expected!(lex, "function name"),
            );
            // eat!(lex,
            //     Tt::Lparen => skip::balanced_parens(lex, 1)?,
            //     _ => expected!(lex, "formal parameter list"),
            // );
            // eat!(lex,
            //     Tt::Lbrace => skip::balanced_braces(lex, 1)?,
            //     _ => expected!(lex, "function body"),
            // );

            let here = lex.here();
            let end_pos = here.span.start - here.ws_before.len();
            source.push_str(&lex.input()[start_pos..end_pos]);

            Ok(Export::Named(vec![ExportSpec::same(name)]))
        },
        _ => expected!(lex, "keyword 'default' or '*' or '{' or declaration"),
    )
}

#[inline(always)]
fn parse_import<'f, 's>(
    lex: &mut lex::Lexer<'f, 's>,
    source: &mut String,
) -> Result<ParsedImport<'s>> {
    #[inline(always)]
    fn parse_binds<'f, 's>(
        lex: &mut lex::Lexer<'f, 's>,
        source: &mut String,
        binds: &mut Bindings<'s>,
        expected: &'static str,
    ) -> Result<()> {
        eat!(lex => tok { source.push_str(tok.ws_before) },
            Tt::Star => eat!(lex => tok { source.push_str(tok.ws_before) },
                Tt::Id("as") => eat!(lex => tok { source.push_str(tok.ws_before) },
                    Tt::Id(name_space) => {
                        *binds = Bindings::NameSpace(name_space)
                    },
                    _ => expected!(lex, "name space binding name"),
                ),
                _ => expected!(lex, "keyword 'as'"),
            ),
            Tt::Lbrace => {
                let mut imports = Vec::new();
                loop {
                    eat!(lex => tok { source.push_str(tok.ws_before) },
                        Tt::Id(_) |
                        Tt::Default => {
                            let name = tok.tt.as_str();
                            eat!(lex => tok { source.push_str(tok.ws_before) },
                                Tt::Id("as") => eat!(lex => tok { source.push_str(tok.ws_before) },
                                    // we don't need | Tt::Default here since it is always a binding name
                                    Tt::Id(bind) => {
                                        imports.push(ImportSpec::new(name, bind));
                                        eat!(lex => tok { source.push_str(tok.ws_before) },
                                            Tt::Rbrace => break,
                                            Tt::Comma => {},
                                            _ => expected!(lex, "',' or '}'"),
                                        );
                                    },
                                    _ => expected!(lex, "binding name after keyword 'as'"),
                                ),
                                Tt::Rbrace => {
                                    imports.push(ImportSpec::same(name));
                                    break
                                },
                                Tt::Comma => {
                                    imports.push(ImportSpec::same(name));
                                },
                                _ => expected!(lex, "',' or '}' or keyword 'as'"),
                            )
                        },
                        Tt::Rbrace => break,
                        _ => expected!(lex, "import specifier or '}'"),
                    );
                }
                *binds = Bindings::Named(imports);
            },
            _ => expected!(lex, expected),
        );
        Ok(())
    }

    let mut default_bind = None;
    let mut binds = Bindings::None;

    eat!(lex => tok { source.push_str(tok.ws_before) },
        Tt::StrLitSgl(module_source) |
        Tt::StrLitDbl(module_source) => {
            return Ok(ParsedImport::Import(Import::new(module_source, match lex::str_lit_value(module_source) {
                Ok(module) => module,
                Err(error) => return Err(Error {
                    kind: ErrorKind::ParseStrLitError(error),
                    span: lex.recover_span(tok.span).with_owned(),
                }),
            })))
        },
        Tt::Id(default) => {
            default_bind = Some(default);
            eat!(lex => tok { source.push_str(tok.ws_before) },
                Tt::Comma => parse_binds(lex, source, &mut binds, "bindings")?,
                _ => {},
            );
        },
        Tt::Dot => {
            eat!(lex => tok { source.push_str(tok.ws_before) },
                Tt::Id("meta") => {
                    source.push_str("__import_meta");
                    return Ok(ParsedImport::ImportMeta)
                },
                _ => {
                    expected!(lex, "keyword 'meta'");
                },
            );
        },
        _ => parse_binds(lex, source, &mut binds, "module name (string literal) or bindings")?,
    );
    eat!(lex => tok { source.push_str(tok.ws_before) },
        Tt::Id("from") => {},
        _ => expected!(lex, "keyword 'from'"),
    );
    eat!(lex => tok { source.push_str(tok.ws_before) },
        Tt::StrLitSgl(module_source) |
        Tt::StrLitDbl(module_source) => {
            Ok(ParsedImport::Import(Import {
                module_source,
                module: match lex::str_lit_value(module_source) {
                    Ok(module) => module,
                    Err(error) => return Err(Error {
                        kind: ErrorKind::ParseStrLitError(error),
                        span: lex.recover_span(tok.span).with_owned(),
                    }),
                },
                default_bind,
                binds,
            }))
        },
        _ => expected!(lex, "module name (string literal)"),
    )
}

#[cfg(test)]
mod test {
    use super::*;
    use esparse::lex;

    macro_rules! assert_export_form {
        ($source:expr, $result:expr, $out:expr $(,)*) => {{
            let mut lexer = lex::Lexer::new_unnamed($source);
            assert_eq!(lexer.advance().tt, Tt::Export);
            let mut output = String::new();
            assert_eq!(parse_export(&mut lexer, &mut output).unwrap(), $result);
            assert_eq!(output, $out);
        }};
    }

    macro_rules! assert_export_form_err {
        ($source:expr $(,)*) => {{
            let mut lexer = lex::Lexer::new_unnamed($source);
            assert_eq!(lexer.advance().tt, Tt::Export);
            let mut output = String::new();
            parse_export(&mut lexer, &mut output).unwrap_err();
        }};
    }

    #[test]
    fn test_export_default() {
        assert_export_form!(
            "export //\ndefault /* comment */ 0 _next",
            Export::Default("__default"),
            // " //\n /* comment */ 0",
            " //\nconst __default = ",
        );
        assert_export_form!(
            "export default class Test {} _next",
            Export::Default("Test"),
            // "  class Test {}",
            "  class Test",
        );
        assert_export_form!(
            "export default function test() {} _next",
            Export::Default("test"),
            // "  function test() {}",
            "  function test",
        );
        assert_export_form!(
            "export default function* testGen() {} _next",
            Export::Default("testGen"),
            // "  function* testGen() {}",
            "  function* testGen",
        );
        assert_export_form!(
            "export default async function testAsync() {} _next",
            Export::Default("testAsync"),
            // "  async function testAsync() {}",
            "  async function testAsync",
        );
        assert_export_form!(
            "export default async function* testAsyncGen() {} _next",
            Export::Default("testAsyncGen"),
            // "  async function* testAsyncGen() {}",
            "  async function* testAsyncGen",
        );
        assert_export_form!(
            "export default async _next",
            Export::Default("__default"),
            // "  async function* testAsyncGen() {}",
            " const __default =  async",
        );
        assert_export_form!(
            "export default async\nfunction not() {} _next",
            Export::Default("__default"),
            // "  async function* testAsyncGen() {}",
            " const __default =  async",
        );
    }

    #[test]
    fn test_export_default_err() {
        assert_export_form_err!("export default class {} _next");
        assert_export_form_err!("export default function() {} _next");
        assert_export_form_err!("export default function*() {} _next");
        assert_export_form_err!("export default async function() {} _next");
        assert_export_form_err!("export default async function*() {} _next");
    }

    #[test]
    fn test_export_binding() {
        assert_export_form!(
            "export var asdf _next",
            Export::Named(vec![ExportSpec::same("asdf")]),
            " var asdf",
        );
        assert_export_form!(
            "export let a = 1, b = (1, 2), c = 3, d = (za, zb) => b, e _next",
            Export::Named(vec![
                ExportSpec::same("a"),
                ExportSpec::same("b"),
                ExportSpec::same("c"),
                ExportSpec::same("d"),
                ExportSpec::same("e"),
            ]),
            " let a = 1, b = (1, 2), c = 3, d = (za, zb) => b, e",
        );
        assert_export_form!(
            "export const j = class A extends B(c, d) {}, k = 1 _next",
            Export::Named(vec![ExportSpec::same("j"), ExportSpec::same("k"),]),
            " const j = class A extends B(c, d) {}, k = 1",
        );
    }

    #[test]
    fn test_export_hoistable_declaration() {
        assert_export_form!(
            "export class Test2 {} _next",
            Export::Named(vec![ExportSpec::same("Test2")]),
            // " class Test2 {}",
            " class Test2",
        );
        assert_export_form!(
            "export function test2() {} _next",
            Export::Named(vec![ExportSpec::same("test2")]),
            // " function test2() {}",
            " function test2",
        );
        assert_export_form!(
            "export function* testGen2() {} _next",
            Export::Named(vec![ExportSpec::same("testGen2")]),
            // " function* testGen2() {}",
            " function* testGen2",
        );
        assert_export_form!(
            "export async function asyncTest2() {} _next",
            Export::Named(vec![ExportSpec::same("asyncTest2")]),
            // " function asyncTest2() {}",
            " async function asyncTest2",
        );
        assert_export_form!(
            "export async function* asyncTestGen2() {} _next",
            Export::Named(vec![ExportSpec::same("asyncTestGen2")]),
            // " function* asyncTestGen2() {}",
            " async function* asyncTestGen2",
        );
    }

    #[test]
    fn test_export_hoistable_declaration_err() {
        assert_export_form_err!("export async\nfunction not() {} _next");
    }

    #[test]
    fn test_export_list() {
        assert_export_form!(
            "export {va as vaz, vb, something as default} _next",
            Export::Named(vec![
                ExportSpec::new("va", "vaz"),
                ExportSpec::same("vb"),
                ExportSpec::new("something", "default"),
            ]),
            "       ",
        );
    }

    #[test]
    fn test_export_ns_from() {
        assert_export_form!(
            "export * from 'a_module' _next",
            Export::AllFrom("'a_module'", Cow::Borrowed("a_module")),
            "   ",
        );
    }

    #[test]
    fn test_export_list_from() {
        assert_export_form!(
            "export {va as vaz, vb, something as default, default as something_else, default, default as default} from 'a_module' _next",
            Export::NamedFrom(vec![
                ExportSpec::new("va", "vaz"),
                ExportSpec::same("vb"),
                ExportSpec::new("something", "default"),
                ExportSpec::new("default", "something_else"),
                ExportSpec::same("default"),
                ExportSpec::same("default"),
            ], "'a_module'", Cow::Borrowed("a_module")),
            "                ",
        );
    }

    macro_rules! assert_import_form {
        ($source:expr, $result:expr, $out:expr $(,)*) => {{
            let mut lexer = lex::Lexer::new_unnamed($source);
            assert_eq!(lexer.advance().tt, Tt::Import);
            let mut output = String::new();
            assert_eq!(parse_import(&mut lexer, &mut output).unwrap(), $result);
            assert_eq!(output, $out);
        }};
    }

    #[test]
    fn test_import_bare() {
        assert_import_form!(
            "import 'a_module' _next",
            ParsedImport::Import(Import {
                module_source: "'a_module'",
                module: Cow::Borrowed("a_module"),
                default_bind: None,
                binds: Bindings::None,
            }),
            " ",
        );
        assert_import_form!(
            "import \"a_module\" _next",
            ParsedImport::Import(Import {
                module_source: "\"a_module\"",
                module: Cow::Borrowed("a_module"),
                default_bind: None,
                binds: Bindings::None,
            }),
            " ",
        );
    }

    #[test]
    fn test_import_default() {
        assert_import_form!(
            "import test from 'a_module' _next",
            ParsedImport::Import(Import {
                module_source: "'a_module'",
                module: Cow::Borrowed("a_module"),
                default_bind: Some("test"),
                binds: Bindings::None,
            }),
            "   ",
        );
        assert_import_form!(
            "import test from \"a_module\" _next",
            ParsedImport::Import(Import {
                module_source: "\"a_module\"",
                module: Cow::Borrowed("a_module"),
                default_bind: Some("test"),
                binds: Bindings::None,
            }),
            "   ",
        );
    }

    #[test]
    fn test_import_name_space() {
        assert_import_form!(
            "import * as test from 'a_module' _next",
            ParsedImport::Import(Import {
                module_source: "'a_module'",
                module: Cow::Borrowed("a_module"),
                default_bind: None,
                binds: Bindings::NameSpace("test"),
            }),
            "     ",
        );
        assert_import_form!(
            "import * as test from \"a_module\" _next",
            ParsedImport::Import(Import {
                module_source: "\"a_module\"",
                module: Cow::Borrowed("a_module"),
                default_bind: None,
                binds: Bindings::NameSpace("test"),
            }),
            "     ",
        );
    }

    #[test]
    fn test_import_named() {
        assert_import_form!(
            "import { } from 'a_module' _next",
            ParsedImport::Import(Import {
                module_source: "'a_module'",
                module: Cow::Borrowed("a_module"),
                default_bind: None,
                binds: Bindings::Named(vec![]),
            }),
            "    ",
        );
        assert_import_form!(
            "import { } from \"a_module\" _next",
            ParsedImport::Import(Import {
                module_source: "\"a_module\"",
                module: Cow::Borrowed("a_module"),
                default_bind: None,
                binds: Bindings::Named(vec![]),
            }),
            "    ",
        );
        assert_import_form!(
            "import { name } from 'a_module' _next",
            ParsedImport::Import(Import {
                module_source: "'a_module'",
                module: Cow::Borrowed("a_module"),
                default_bind: None,
                binds: Bindings::Named(vec![ImportSpec::same("name"),]),
            }),
            "     ",
        );
        assert_import_form!(
            "import { name , } from 'a_module' _next",
            ParsedImport::Import(Import {
                module_source: "'a_module'",
                module: Cow::Borrowed("a_module"),
                default_bind: None,
                binds: Bindings::Named(vec![ImportSpec::same("name"),]),
            }),
            "      ",
        );
        assert_import_form!(
            "import { name , another } from 'a_module' _next",
            ParsedImport::Import(Import {
                module_source: "'a_module'",
                module: Cow::Borrowed("a_module"),
                default_bind: None,
                binds: Bindings::Named(
                    vec![ImportSpec::same("name"), ImportSpec::same("another"),]
                ),
            }),
            "       ",
        );
        assert_import_form!(
            "import { name as thing } from 'a_module' _next",
            ParsedImport::Import(Import {
                module_source: "'a_module'",
                module: Cow::Borrowed("a_module"),
                default_bind: None,
                binds: Bindings::Named(vec![ImportSpec::new("name", "thing"),]),
            }),
            "       ",
        );
        assert_import_form!(
            "import { name as thing , } from 'a_module' _next",
            ParsedImport::Import(Import {
                module_source: "'a_module'",
                module: Cow::Borrowed("a_module"),
                default_bind: None,
                binds: Bindings::Named(vec![ImportSpec::new("name", "thing"),]),
            }),
            "        ",
        );
        assert_import_form!(
            "import { name as thing , another , third as one } from 'a_module' _next",
            ParsedImport::Import(Import {
                module_source: "'a_module'",
                module: Cow::Borrowed("a_module"),
                default_bind: None,
                binds: Bindings::Named(vec![
                    ImportSpec::new("name", "thing"),
                    ImportSpec::same("another"),
                    ImportSpec::new("third", "one"),
                ]),
            }),
            "             ",
        );
    }

    #[test]
    fn test_import_default_named() {
        assert_import_form!(
            "import test , { } from 'a_module' _next",
            ParsedImport::Import(Import {
                module_source: "'a_module'",
                module: Cow::Borrowed("a_module"),
                default_bind: Some("test"),
                binds: Bindings::Named(vec![]),
            }),
            "      ",
        );
        assert_import_form!(
            "import test , { name as thing , another , third as one } from 'a_module' _next",
            ParsedImport::Import(Import {
                module_source: "'a_module'",
                module: Cow::Borrowed("a_module"),
                default_bind: Some("test"),
                binds: Bindings::Named(vec![
                    ImportSpec::new("name", "thing"),
                    ImportSpec::same("another"),
                    ImportSpec::new("third", "one"),
                ]),
            }),
            "               ",
        );
    }

    #[test]
    fn test_import_default_name_space() {
        assert_import_form!(
            "import def , * as test from 'a_module' _next",
            ParsedImport::Import(Import {
                module_source: "'a_module'",
                module: Cow::Borrowed("a_module"),
                default_bind: Some("def"),
                binds: Bindings::NameSpace("test"),
            }),
            "       ",
        );
        assert_import_form!(
            "import def , * as test from \"a_module\" _next",
            ParsedImport::Import(Import {
                module_source: "\"a_module\"",
                module: Cow::Borrowed("a_module"),
                default_bind: Some("def"),
                binds: Bindings::NameSpace("test"),
            }),
            "       ",
        );
    }
}
