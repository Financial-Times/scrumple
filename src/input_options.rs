use fnv::FnvHashSet;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum PackageManager {
    Bower,
    Npm,
}

impl PackageManager {
    pub fn dir(&self) -> &'static str {
        match self {
            Self::Bower => "bower_components",
            Self::Npm => "node_modules",
        }
    }

    pub fn file(&self) -> &'static str {
        match self {
            Self::Bower => ".bower.json",
            Self::Npm => "package.json",
        }
    }
}

impl Default for PackageManager {
    fn default() -> PackageManager {
        PackageManager::Npm
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InputOptions {
    pub package_manager: PackageManager,
    pub external: FnvHashSet<String>,
}
