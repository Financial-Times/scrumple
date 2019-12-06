use crate::resolver::Resolved;
use fnv::FnvHashMap;

#[derive(Debug)]
pub struct Source {
    pub prefix: String,
    pub body: String,
    pub suffix: String,
    pub original: Option<String>,
}

#[derive(Debug)]
pub struct ModuleInfo {
    pub source: Source,
    pub deps: Vec<String>,
}

#[derive(Debug)]
pub struct Module {
    pub source: Source,
    pub deps: FnvHashMap<String, Resolved>,
}

#[derive(Debug)]
pub enum ModuleState {
    Loading,
    Loaded(Module),
}

impl ModuleState {
    fn expect(self, message: &str) -> Module {
        match self {
            ModuleState::Loading => panic!("{}", message),
            ModuleState::Loaded(module) => module,
        }
    }
    pub fn unwrap(self) -> Module {
        self.expect("unwrapped ModuleState that was still loading")
    }
}
