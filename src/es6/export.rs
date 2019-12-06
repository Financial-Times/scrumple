use std::borrow::Cow;

#[derive(Debug, PartialEq, Eq)]
pub enum Export<'s> {
    Default(&'s str),
    AllFrom(&'s str, Cow<'s, str>),
    Named(Vec<ExportSpec<'s>>),
    NamedFrom(Vec<ExportSpec<'s>>, &'s str, Cow<'s, str>),
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExportSpec<'s> {
    pub bind: &'s str,
    pub name: &'s str,
}

impl<'s> ExportSpec<'s> {
    #[inline]
    pub fn new(bind: &'s str, name: &'s str) -> Self {
        ExportSpec { name, bind }
    }

    #[inline]
    pub fn same(name: &'s str) -> Self {
        ExportSpec::new(name, name)
    }
}
