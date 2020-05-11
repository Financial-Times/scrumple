use crate::path_ext::*;
use crate::CliError;
use fnv::FnvHashMap;
use matches::matches;
use serde::de::{SeqAccess, Visitor};
use serde::{de, Deserialize, Deserializer};
use std::cell::RefCell;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::{fmt, fs, io, mem};

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum BrowserSubstitution<T> {
    Ignore,
    Replace(T),
}

#[derive(Debug, Default, Deserialize, PartialEq, Eq, Clone)]
#[serde(transparent)]
pub struct BrowserSubstitutionMap(pub FnvHashMap<PathBuf, BrowserSubstitution<PathBuf>>);

#[derive(Debug, Clone, Default)]
pub struct PackageCache {
    pub pkgs: RefCell<FnvHashMap<PathBuf, Option<Rc<PackageInfo>>>>,
}

impl PackageCache {
    pub fn nearest_package_info(
        &self,
        mut dir: PathBuf,
        package_manager: crate::input_options::PackageManager,
    ) -> Result<Option<Rc<PackageInfo>>, CliError> {
        loop {
            if !matches!(dir.file_name(), Some(s) if s == package_manager.dir()) {
                if let Some(info) = self.package_info(&mut dir, package_manager)? {
                    return Ok(Some(info));
                }
            }
            if !dir.pop() {
                return Ok(None);
            }
        }
    }

    pub fn package_info(
        &self,
        dir: &mut PathBuf,
        package_manager: crate::input_options::PackageManager,
    ) -> Result<Option<Rc<PackageInfo>>, CliError> {
        let mut pkgs = self.pkgs.borrow_mut();
        let manifest_file_names = package_manager.files();
        Ok(pkgs
            .entry(dir.clone())
            .or_insert_with(|| {
                for manifest_file_name in manifest_file_names {
                    dir.push(manifest_file_name);
                    if let Ok(file) = fs::File::open(&dir) {
                        let buf_reader = io::BufReader::new(file);
                        if let Ok(mut info) = serde_json::from_reader::<_, PackageInfo>(buf_reader)
                        {
                            dir.pop();
                            info.set_base(&dir);
                            return Some(Rc::new(info));
                        } else {
                            dir.pop();
                        }
                    } else {
                        dir.pop();
                    }
                }
                None
            })
            .as_ref()
            .cloned())
    }
}

#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct PackageInfo {
    pub main: PathBuf,
    pub browser_substitutions: BrowserSubstitutionMap,
}

impl PackageInfo {
    fn set_base(&mut self, base: &Path) {
        self.main.prepend_resolving(base);
        let substs = mem::replace(&mut self.browser_substitutions, Default::default());
        self.browser_substitutions
            .0
            .extend(substs.0.into_iter().map(|(mut from, mut to)| {
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
            #[serde(deserialize_with = "from_main")]
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

#[macro_export]
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
            BrowserField::Empty => Default::default(),
            BrowserField::Main(mut to) => {
                if !to.is_explicitly_relative() {
                    to.prepend_resolving(Path::new("."));
                }
                BrowserSubstitutionMap(map! {
                    PathBuf::from(".") => BrowserSubstitution::Replace(to.clone()),
                    main.to_owned() => BrowserSubstitution::Replace(to.clone()),
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
    where
        D: Deserializer<'de>,
    {
        struct BrowserFieldVisitor;

        impl<'de> Visitor<'de> for BrowserFieldVisitor {
            type Value = BrowserField;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "anything at all")
            }
            fn visit_map<A: de::MapAccess<'de>>(self, access: A) -> Result<Self::Value, A::Error> {
                Ok(BrowserField::Complex(Deserialize::deserialize(
                    de::value::MapAccessDeserializer::new(access),
                )?))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(BrowserField::Main(PathBuf::from(v)))
            }

            visit_unconditionally!('de BrowserField::Empty, bool i64 i128 u64 u128 f64 bytes none some unit newtype_struct seq enum);
        }

        deserializer.deserialize_any(BrowserFieldVisitor)
    }
}

// The main might be a string or an array of strings
fn from_main<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    for<'a> T: From<&'a str> + Deserialize<'de>,
    D: Deserializer<'de>,
{
    struct FromMain<T>(PhantomData<T>);

    impl<'de, T> Visitor<'de> for FromMain<T>
    where
        for<'a> T: From<&'a str> + Deserialize<'de>,
    {
        type Value = Option<T>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "string or array or nothing")
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            Ok(Some(T::from(v)))
        }

        fn visit_seq<S>(self, mut seq: S) -> Result<Self::Value, S::Error>
        where
            S: SeqAccess<'de>,
        {
            let mut value: Option<T> = None;
            while let Ok(item) = seq.next_element::<String>() {
                if let Some(item) = item {
                    if item.ends_with(".js") {
                        value = Some(T::from(&item));
                    }
                } else {
                    break;
                }
            }
            Ok(value)
        }

        visit_unconditionally!('de None, bool i64 i128 u64 u128 f64 bytes none some unit newtype_struct map enum);
    }

    deserializer.deserialize_any(FromMain(PhantomData))
}

impl<'de, T> Deserialize<'de> for BrowserSubstitution<T>
where
    for<'a> T: From<&'a str>,
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct SubstitutionVisitor<T>(PhantomData<T>);

        impl<'de, T> Visitor<'de> for SubstitutionVisitor<T>
        where
            for<'a> T: From<&'a str>,
        {
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
mod test {
    use super::*;
    use matches::assert_matches;

    #[test]
    fn test_deserialize_browser_subst() {
        let parse = serde_json::from_str::<BrowserSubstitution<String>>;
        assert_matches!(parse("null"), Err(_));
        assert_matches!(parse("100"), Err(_));
        assert_matches!(parse("[1, 2, 3]"), Err(_));
        assert_matches!(parse("false"), Ok(BrowserSubstitution::Ignore));
        assert_matches!(parse("true"), Err(_));
        assert_eq!(
            parse(r#""asdf""#).unwrap(),
            BrowserSubstitution::Replace("asdf".to_owned())
        );
        assert_eq!(
            parse(r#""""#).unwrap(),
            BrowserSubstitution::Replace("".to_owned())
        );
    }

    #[test]
    fn test_deserialize_browser() {
        let parse = serde_json::from_str::<BrowserSubstitutionMap>;
        assert_matches!(parse(r#"null"#), Err(_));
        assert_matches!(parse(r#""simple.browser.js""#), Err(_));
        assert_eq!(parse(r#"{}"#).unwrap(), BrowserSubstitutionMap(map! {}));
        assert_eq!(
            parse(r#"{"mod": "dom"}"#).unwrap(),
            BrowserSubstitutionMap(map! {
                PathBuf::from("mod") => BrowserSubstitution::Replace(PathBuf::from("dom")),
            })
        );
        assert_eq!(
            parse(r#"{"./file.js": "./file.browser.js"}"#).unwrap(),
            BrowserSubstitutionMap(map! {
                PathBuf::from("./file.js") => BrowserSubstitution::Replace(PathBuf::from("./file.browser.js")),
            })
        );
        assert_eq!(
            parse(r#"{"ignore": false}"#).unwrap(),
            BrowserSubstitutionMap(map! {
                PathBuf::from("ignore") => BrowserSubstitution::Ignore,
            })
        );
        assert_eq!(
            parse(
                r#"{
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
    }"#
            )
            .unwrap(),
            BrowserSubstitutionMap(map! {
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
            })
        );
    }

    #[test]
    fn test_deserialize_package_info() {
        let parse = serde_json::from_str::<PackageInfo>;
        assert_matches!(parse("null"), Err(_));
        assert_matches!(parse("100"), Err(_));
        assert_matches!(parse("[1, 2, 3]"), Err(_));
        assert_eq!(
            parse(r#"{}"#).unwrap(),
            PackageInfo {
                main: PathBuf::from("./index"),
                browser_substitutions: BrowserSubstitutionMap(map! {}),
            }
        );
        assert_eq!(
            parse(r#"{"browser": null}"#).unwrap(),
            PackageInfo {
                main: PathBuf::from("./index"),
                browser_substitutions: BrowserSubstitutionMap(map! {}),
            }
        );
        assert_eq!(
            parse(r#"{"browser": "simple"}"#).unwrap(),
            PackageInfo {
                main: PathBuf::from("./index"),
                browser_substitutions: BrowserSubstitutionMap(map! {
                    PathBuf::from(".") => BrowserSubstitution::Replace(PathBuf::from("./simple")),
                }),
            }
        );
        assert_eq!(
            parse(r#"{"browser": {}}"#).unwrap(),
            PackageInfo {
                main: PathBuf::from("./index"),
                browser_substitutions: BrowserSubstitutionMap(map! {}),
            }
        );
        assert_eq!(
            parse(r#"{"browser": {"mod": false}}"#).unwrap(),
            PackageInfo {
                main: PathBuf::from("./index"),
                browser_substitutions: BrowserSubstitutionMap(map! {
                    PathBuf::from("mod") => BrowserSubstitution::Ignore,
                }),
            }
        );
    }
}
