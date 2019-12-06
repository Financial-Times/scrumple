use crate::input_options::InputOptions;
use crate::manifest::{BrowserSubstitution, PackageCache, PackageInfo};
use crate::path_ext::*;
use crate::CliError;
use matches::matches;
use std::path::{self, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
enum ModuleSubstitution {
    Normal,
    Ignore,
    External,
    Replace(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PathSubstitution {
    Missing,
    Normal,
    Ignore,
    Replace(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    External,
    Ignore,
    // CoreWithSubst(PathBuf),
    Normal(PathBuf),
}

#[derive(Debug, Clone, Default)]
pub struct Resolver {
    input_options: InputOptions,
    pub cache: PackageCache,
}

impl Resolver {
    pub fn new(input_options: InputOptions) -> Self {
        Resolver {
            input_options,
            ..Default::default()
        }
    }

    #[inline]
    fn needs_dir(name: &str, path: &Path) -> bool {
        name.ends_with('/')
            || matches!(
                path.components().last(),
                Some(path::Component::CurDir) | Some(path::Component::ParentDir)
            )
    }

    pub fn resolve_main(&self, mut dir: PathBuf, name: &str) -> Result<Resolved, CliError> {
        let path = Path::new(name);
        dir.append_resolving(path);
        let needs_dir = Self::needs_dir(name, path);
        self.resolve_path_or_module(None, dir, needs_dir, false)?
            .ok_or_else(|| CliError::MainNotFound {
                name: name.to_owned(),
            })
    }

    pub fn resolve(&self, context: &Path, name: &str) -> Result<Resolved, CliError> {
        if name.is_empty() {
            return Err(CliError::EmptyModuleName {
                context: context.to_owned(),
            });
        }

        let path = Path::new(name);
        let needs_dir = Self::needs_dir(name, path);
        if path.is_absolute() {
            Ok(self
                .resolve_path_or_module(Some(context), path.to_owned(), needs_dir, false)?
                .ok_or_else(|| CliError::ModuleNotFound {
                    context: context.to_owned(),
                    name: name.to_owned(),
                })?)
        } else if path.is_explicitly_relative() {
            let mut dir = context.to_owned();
            let did_pop = dir.pop(); // to directory
            debug_assert!(did_pop);
            dir.append_resolving(path);
            Ok(self
                .resolve_path_or_module(Some(context), dir, needs_dir, false)?
                .ok_or_else(|| CliError::ModuleNotFound {
                    context: context.to_owned(),
                    name: name.to_owned(),
                })?)
        } else {
            match self.module_substitution(context, name)? {
                ModuleSubstitution::Ignore => return Ok(Resolved::Ignore),
                ModuleSubstitution::External => return Ok(Resolved::External),
                ModuleSubstitution::Replace(new_name) => {
                    // TODO: detect cycles
                    // eprintln!("module replace {} => {}", name, &new_name);
                    return self.resolve(context, &new_name);
                }
                ModuleSubstitution::Normal => {}
            }

            let component_dir = self.input_options.package_manager.dir();

            let mut suffix = PathBuf::from(component_dir);
            for part in path.components() {
                suffix.push(part);
            }

            let mut dir = context.to_owned();
            while dir.pop() {
                match dir.file_name() {
                    Some(s) if s == component_dir => continue,
                    _ => {}
                }
                let new_path = dir.join(&suffix);
                if let Some(result) =
                    self.resolve_path_or_module(Some(context), new_path, needs_dir, false)?
                {
                    return Ok(result);
                }
            }

            Err(CliError::ModuleNotFound {
                context: context.to_owned(),
                name: name.to_owned(),
            })
        }
    }

    fn module_substitution(
        &self,
        context: &Path,
        name: &str,
    ) -> Result<ModuleSubstitution, CliError> {
        let module_name = name.split('/').next().unwrap();
        if self.input_options.external.contains(module_name) {
            return Ok(ModuleSubstitution::External);
        }
        if let Some(p) = context.parent() {
            if let Some(info) = self
                .cache
                .nearest_package_info(p.to_owned(), self.input_options.package_manager)?
            {
                match info.browser_substitutions.0.get(Path::new(module_name)) {
                    Some(&BrowserSubstitution::Ignore) => return Ok(ModuleSubstitution::Ignore),
                    Some(&BrowserSubstitution::Replace(ref to)) => {
                        let mut new_name = to.to_string_lossy().into_owned();
                        new_name.push_str(&name[module_name.len()..]);
                        return Ok(ModuleSubstitution::Replace(new_name));
                    }
                    None => {}
                }
            }
        }
        Ok(ModuleSubstitution::Normal)
    }

    pub fn resolve_path_or_module(
        &self,
        context: Option<&Path>,
        mut path: PathBuf,
        needs_dir: bool,
        package: bool,
    ) -> Result<Option<Resolved>, CliError> {
        let package_info = self
            .cache
            .nearest_package_info(path.clone(), self.input_options.package_manager)?;

        macro_rules! check_path {
            ( $package_info:ident, $path:ident ) => {
                // eprintln!("check {}", $path.display());
                match Self::check_path($package_info.as_ref().map(|x| x.as_ref()), &$path) {
                    PathSubstitution::Normal => {
                        // eprintln!("resolve {}", $path.display());
                        return Ok(Some(Resolved::Normal($path)));
                    }
                    PathSubstitution::Ignore => return Ok(Some(Resolved::Ignore)),
                    PathSubstitution::Replace(p) => {
                        // eprintln!("path replace {} => {}", $path.display(), p.display());
                        return Ok(Some(Resolved::Normal(p)));
                    }
                    PathSubstitution::Missing => {}
                }
            };
        }

        if !needs_dir {
            // <path>
            check_path!(package_info, path);

            let file_name = path
                .file_name()
                .ok_or_else(|| CliError::RequireRoot {
                    context: context.map(|p| p.to_owned()),
                    path: path.clone(),
                })?
                .to_owned();

            let mut new_file_name = file_name.to_owned();

            // <path>.mjs
            new_file_name.push(".mjs");
            path.set_file_name(&new_file_name);
            check_path!(package_info, path);
            new_file_name.clear();
            new_file_name.push(&file_name);

            // <path>.js
            new_file_name.push(".js");
            path.set_file_name(&new_file_name);
            check_path!(package_info, path);

            // <path>.json
            new_file_name.push("on"); // .js|on
            path.set_file_name(&new_file_name);
            check_path!(package_info, path);

            path.set_file_name(&file_name);
        }

        if !package {
            if let Some(info) = self
                .cache
                .package_info(&mut path, self.input_options.package_manager)?
            {
                path.replace_with(&info.main);
                return self.resolve_path_or_module(context, path, false, true);
            }
        }

        // <path>/index.mjs
        path.push("index.mjs");
        check_path!(package_info, path);
        path.pop();

        // <path>/index.js
        path.push("index.js");
        check_path!(package_info, path);
        path.pop();

        // <path>/index.json
        path.push("index.json");
        check_path!(package_info, path);
        // path.pop();

        Ok(None)
    }

    fn check_path(package_info: Option<&PackageInfo>, path: &Path) -> PathSubstitution {
        if let Some(package_info) = package_info {
            match package_info.browser_substitutions.0.get(path) {
                Some(BrowserSubstitution::Ignore) => return PathSubstitution::Ignore,
                Some(BrowserSubstitution::Replace(ref path)) => {
                    return PathSubstitution::Replace(path.clone());
                }
                None => {}
            }
        }
        if path.is_file() {
            PathSubstitution::Normal
        } else {
            PathSubstitution::Missing
        }
    }
}
