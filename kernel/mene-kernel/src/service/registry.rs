#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryError {
    InvalidName,
    InvalidHandle,
    NotFound,
    AlreadyExists,
    RegistryFull,
}

#[derive(Clone, Copy)]
pub struct ServiceHandle(pub usize);

impl ServiceHandle {
    pub const fn new(handle: usize) -> Self {
        Self(handle)
    }

    pub const fn as_usize(&self) -> usize {
        self.0
    }
}

const MAX_SERVICES: usize = 32;
const MAX_NAME_LEN: usize = 32;

#[derive(Clone, Copy)]
struct RegistryEntry {
    in_use: bool,
    owner_pid: usize,
    handle: usize,
    name_len: usize,
    name: [u8; MAX_NAME_LEN],
}

impl RegistryEntry {
    const fn empty() -> Self {
        Self {
            in_use: false,
            owner_pid: 0,
            handle: 0,
            name_len: 0,
            name: [0; MAX_NAME_LEN],
        }
    }

    fn matches(&self, name: &[u8]) -> bool {
        self.in_use && self.name_len == name.len() && self.name[..self.name_len] == *name
    }
}

pub struct ServiceRegistry {
    entries: [RegistryEntry; MAX_SERVICES],
}

impl ServiceRegistry {
    pub const fn new() -> Self {
        Self {
            entries: [RegistryEntry::empty(); MAX_SERVICES],
        }
    }

    pub fn register(&mut self, name: &[u8], owner_pid: usize, handle: ServiceHandle) -> Result<(), RegistryError> {
        if name.is_empty() || name.len() > MAX_NAME_LEN {
            return Err(RegistryError::InvalidName);
        }
        if handle.as_usize() == 0 {
            return Err(RegistryError::InvalidHandle);
        }

        for entry in self.entries.iter_mut() {
            if entry.matches(name) {
                entry.owner_pid = owner_pid;
                entry.handle = handle.as_usize();
                return Ok(());
            }
        }

        for entry in self.entries.iter_mut() {
            if !entry.in_use {
                entry.in_use = true;
                entry.owner_pid = owner_pid;
                entry.handle = handle.as_usize();
                entry.name_len = name.len();
                entry.name[..name.len()].copy_from_slice(name);
                return Ok(());
            }
        }

        Err(RegistryError::RegistryFull)
    }

    pub fn lookup(&self, name: &[u8]) -> Result<ServiceHandle, RegistryError> {
        self.entries
            .iter()
            .find(|e| e.matches(name))
            .map(|e| ServiceHandle::new(e.handle))
            .ok_or(RegistryError::NotFound)
    }

    pub fn unregister(&mut self, name: &[u8]) -> Result<(), RegistryError> {
        for entry in self.entries.iter_mut() {
            if entry.matches(name) {
                *entry = RegistryEntry::empty();
                return Ok(());
            }
        }
        Err(RegistryError::NotFound)
    }
}
