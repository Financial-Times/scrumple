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

    // the manifest files in order of preference:)
    pub fn files(&self) -> Vec<&'static str> {
        match self {
            Self::Bower => vec![".bower.json", "bower.json"],
            Self::Npm => vec!["package.json"],
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
