use matches::matches;
use std::mem;
use std::path::{self, Path, PathBuf};

pub trait PathBufExt {
    fn append_resolving<P: AsRef<Path> + ?Sized>(&mut self, more: &P);
    fn prepend_resolving<P: AsRef<Path> + ?Sized>(&mut self, base: &P);
    fn empty(&mut self);
    fn replace_with<P: AsRef<Path> + ?Sized>(&mut self, that: &P);
    fn as_mut_vec(&mut self) -> &mut Vec<u8>;
}

impl PathBufExt for PathBuf {
    fn append_resolving<P: AsRef<Path> + ?Sized>(&mut self, more: &P) {
        for c in more.as_ref().components() {
            match c {
                path::Component::Prefix(prefix) => {
                    *self = PathBuf::from(prefix.as_os_str().to_owned());
                }
                path::Component::RootDir => {
                    self.push(path::MAIN_SEPARATOR.to_string());
                }
                path::Component::CurDir => {}
                path::Component::ParentDir => {
                    self.pop();
                }
                path::Component::Normal(part) => {
                    self.push(part);
                }
            }
        }
    }
    fn prepend_resolving<P: AsRef<Path> + ?Sized>(&mut self, base: &P) {
        let mut tmp = base.as_ref().to_owned();
        mem::swap(self, &mut tmp);
        self.append_resolving(tmp.as_path());
    }
    fn empty(&mut self) {
        self.as_mut_vec().clear();
    }
    fn replace_with<P: AsRef<Path> + ?Sized>(&mut self, that: &P) {
        self.empty();
        self.push(that);
    }

    fn as_mut_vec(&mut self) -> &mut Vec<u8> {
        unsafe { &mut *(self as *mut PathBuf as *mut Vec<u8>) }
    }
}

pub trait PathExt {
    fn is_explicitly_relative(&self) -> bool;
    fn relative_from<P: AsRef<Path> + ?Sized>(&self, base: &P) -> Option<PathBuf>;
}

impl PathExt for Path {
    #[inline]
    fn is_explicitly_relative(&self) -> bool {
        matches!(
            self.components().next(),
            Some(path::Component::CurDir) | Some(path::Component::ParentDir)
        )
    }
    fn relative_from<P: AsRef<Path> + ?Sized>(&self, base: &P) -> Option<PathBuf> {
        let base = base.as_ref();
        if self.is_absolute() != base.is_absolute() {
            if self.is_absolute() {
                Some(PathBuf::from(self))
            } else {
                None
            }
        } else {
            let mut ita = self.components();
            let mut itb = base.components();
            let mut comps: Vec<path::Component> = vec![];
            loop {
                match (ita.next(), itb.next()) {
                    (None, None) => break,
                    (Some(a), None) => {
                        comps.push(a);
                        comps.extend(ita.by_ref());
                        break;
                    }
                    (None, _) => comps.push(path::Component::ParentDir),
                    (Some(a), Some(b)) if comps.is_empty() && a == b => (),
                    (Some(a), Some(b)) if b == path::Component::CurDir => comps.push(a),
                    (Some(_), Some(b)) if b == path::Component::ParentDir => return None,
                    (Some(a), Some(_)) => {
                        comps.push(path::Component::ParentDir);
                        for _ in itb {
                            comps.push(path::Component::ParentDir);
                        }
                        comps.push(a);
                        comps.extend(ita.by_ref());
                        break;
                    }
                }
            }
            Some(comps.iter().map(|c| c.as_os_str()).collect())
        }
    }
}
