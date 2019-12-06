use std::borrow::Cow;

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Bindings<'s> {
    None,
    NameSpace(&'s str),
    Named(Vec<ImportSpec<'s>>),
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ParsedImport<'s> {
    Import(Import<'s>),
    ImportMeta,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct Import<'s> {
    pub module_source: &'s str,
    pub module: Cow<'s, str>,
    pub default_bind: Option<&'s str>,
    pub binds: Bindings<'s>,
}

impl<'s> Import<'s> {
    #[inline]
    pub fn new(module_source: &'s str, module: Cow<'s, str>) -> Self {
        Import {
            module_source,
            module,
            default_bind: None,
            binds: Bindings::None,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ImportSpec<'s> {
    pub name: &'s str,
    pub bind: &'s str,
}

impl<'s> ImportSpec<'s> {
    #[inline]
    pub fn new(name: &'s str, bind: &'s str) -> Self {
        ImportSpec { name, bind }
    }

    #[inline]
    pub fn same(name: &'s str) -> Self {
        ImportSpec::new(name, name)
    }
}
