//! Process-wide backing store for the Origin Private File System (OPFS).
//!
//! OPFS is scoped by origin and shared by every execution context belonging to
//! that origin. Window and DedicatedWorker therefore cannot keep independent
//! JS Maps: a file created by one realm must immediately be visible to the
//! others. The WebIDL layers keep realm-local wrapper objects while this host
//! registry owns stable entry identities and file bytes.

use deno_core::op2;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const ROOT_ID: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EntryKind {
    Directory,
    File,
}

impl EntryKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Directory => "directory",
            Self::File => "file",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "directory" => Some(Self::Directory),
            "file" => Some(Self::File),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
struct FsEntry {
    id: u32,
    parent: Option<u32>,
    name: String,
    kind: EntryKind,
    bytes: Vec<u8>,
    sequence: u32,
}

#[derive(Debug)]
struct OriginStore {
    entries: HashMap<u32, FsEntry>,
    next_id: u32,
    next_sequence: u32,
}

impl Default for OriginStore {
    fn default() -> Self {
        let mut entries = HashMap::new();
        entries.insert(
            ROOT_ID,
            FsEntry {
                id: ROOT_ID,
                parent: None,
                name: String::new(),
                kind: EntryKind::Directory,
                bytes: Vec::new(),
                sequence: 0,
            },
        );
        Self {
            entries,
            next_id: ROOT_ID + 1,
            next_sequence: 1,
        }
    }
}

fn registry() -> &'static Mutex<HashMap<String, OriginStore>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, OriginStore>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OpfsEntrySnapshot {
    pub status: i32,
    pub id: u32,
    pub parent_id: u32,
    pub name: String,
    pub kind: String,
}

impl OpfsEntrySnapshot {
    fn from_entry(entry: &FsEntry) -> Self {
        Self {
            status: 1,
            id: entry.id,
            parent_id: entry.parent.unwrap_or(0),
            name: entry.name.clone(),
            kind: entry.kind.as_str().to_string(),
        }
    }

    fn missing() -> Self {
        Self {
            status: 0,
            id: 0,
            parent_id: 0,
            name: String::new(),
            kind: String::new(),
        }
    }

    fn type_mismatch() -> Self {
        Self {
            status: -1,
            id: 0,
            parent_id: 0,
            name: String::new(),
            kind: String::new(),
        }
    }
}

fn child_id(store: &OriginStore, parent_id: u32, name: &str) -> Option<u32> {
    store
        .entries
        .values()
        .find(|entry| entry.parent == Some(parent_id) && entry.name == name)
        .map(|entry| entry.id)
}

fn root_impl(origin: String) -> OpfsEntrySnapshot {
    let mut guard = registry().lock().unwrap_or_else(|e| e.into_inner());
    let store = guard.entry(origin).or_default();
    OpfsEntrySnapshot::from_entry(store.entries.get(&ROOT_ID).expect("OPFS root exists"))
}

#[op2]
#[serde]
pub fn op_opfs_root(#[string] origin: String) -> OpfsEntrySnapshot {
    root_impl(origin)
}

fn stat_impl(origin: String, id: u32) -> OpfsEntrySnapshot {
    let guard = registry().lock().unwrap_or_else(|e| e.into_inner());
    guard
        .get(&origin)
        .and_then(|store| store.entries.get(&id))
        .map(OpfsEntrySnapshot::from_entry)
        .unwrap_or_else(OpfsEntrySnapshot::missing)
}

#[op2]
#[serde]
pub fn op_opfs_stat(#[string] origin: String, #[smi] id: u32) -> OpfsEntrySnapshot {
    stat_impl(origin, id)
}

fn get_child_impl(
    origin: String,
    parent_id: u32,
    name: String,
    requested_kind: String,
    create: bool,
) -> OpfsEntrySnapshot {
    let Some(kind) = EntryKind::parse(&requested_kind) else {
        return OpfsEntrySnapshot::type_mismatch();
    };
    let mut guard = registry().lock().unwrap_or_else(|e| e.into_inner());
    let store = guard.entry(origin).or_default();
    let Some(parent) = store.entries.get(&parent_id) else {
        return OpfsEntrySnapshot::missing();
    };
    if parent.kind != EntryKind::Directory {
        return OpfsEntrySnapshot::type_mismatch();
    }

    if let Some(id) = child_id(store, parent_id, &name) {
        let entry = store.entries.get(&id).expect("child id came from store");
        return if entry.kind == kind {
            OpfsEntrySnapshot::from_entry(entry)
        } else {
            OpfsEntrySnapshot::type_mismatch()
        };
    }
    if !create {
        return OpfsEntrySnapshot::missing();
    }

    let id = store.next_id;
    store.next_id += 1;
    let sequence = store.next_sequence;
    store.next_sequence += 1;
    let entry = FsEntry {
        id,
        parent: Some(parent_id),
        name,
        kind,
        bytes: Vec::new(),
        sequence,
    };
    let snapshot = OpfsEntrySnapshot::from_entry(&entry);
    store.entries.insert(id, entry);
    snapshot
}

#[op2]
#[serde]
pub fn op_opfs_get_child(
    #[string] origin: String,
    #[smi] parent_id: u32,
    #[string] name: String,
    #[string] requested_kind: String,
    create: bool,
) -> OpfsEntrySnapshot {
    get_child_impl(origin, parent_id, name, requested_kind, create)
}

fn list_impl(origin: String, parent_id: u32) -> Vec<OpfsEntrySnapshot> {
    let guard = registry().lock().unwrap_or_else(|e| e.into_inner());
    let Some(store) = guard.get(&origin) else {
        return Vec::new();
    };
    let mut entries = store
        .entries
        .values()
        .filter(|entry| entry.parent == Some(parent_id))
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.sequence);
    entries
        .into_iter()
        .map(OpfsEntrySnapshot::from_entry)
        .collect()
}

#[op2]
#[serde]
pub fn op_opfs_list(#[string] origin: String, #[smi] parent_id: u32) -> Vec<OpfsEntrySnapshot> {
    list_impl(origin, parent_id)
}

fn descendants(store: &OriginStore, id: u32) -> Vec<u32> {
    let mut result = Vec::new();
    let mut pending = vec![id];
    while let Some(parent) = pending.pop() {
        for child in store
            .entries
            .values()
            .filter(|entry| entry.parent == Some(parent))
        {
            result.push(child.id);
            pending.push(child.id);
        }
    }
    result
}

/// 1=removed, 0=missing, -1=directory-not-empty.
fn remove_child_impl(origin: String, parent_id: u32, name: String, recursive: bool) -> i32 {
    let mut guard = registry().lock().unwrap_or_else(|e| e.into_inner());
    let Some(store) = guard.get_mut(&origin) else {
        return 0;
    };
    let Some(id) = child_id(store, parent_id, &name) else {
        return 0;
    };
    let nested = descendants(store, id);
    if !recursive && !nested.is_empty() {
        return -1;
    }
    for child in nested {
        store.entries.remove(&child);
    }
    store.entries.remove(&id);
    1
}

#[op2(fast)]
pub fn op_opfs_remove_child(
    #[string] origin: String,
    #[smi] parent_id: u32,
    #[string] name: String,
    recursive: bool,
) -> i32 {
    remove_child_impl(origin, parent_id, name, recursive)
}

fn resolve_impl(origin: String, base_id: u32, target_id: u32) -> Option<Vec<String>> {
    let guard = registry().lock().unwrap_or_else(|e| e.into_inner());
    let store = guard.get(&origin)?;
    if base_id == target_id {
        return Some(Vec::new());
    }
    let mut current = target_id;
    let mut parts = Vec::new();
    loop {
        let entry = store.entries.get(&current)?;
        parts.push(entry.name.clone());
        let parent = entry.parent?;
        if parent == base_id {
            parts.reverse();
            return Some(parts);
        }
        current = parent;
    }
}

#[op2]
#[serde]
pub fn op_opfs_resolve(
    #[string] origin: String,
    #[smi] base_id: u32,
    #[smi] target_id: u32,
) -> Option<Vec<String>> {
    resolve_impl(origin, base_id, target_id)
}

fn read_impl(origin: String, id: u32) -> Vec<u8> {
    let guard = registry().lock().unwrap_or_else(|e| e.into_inner());
    guard
        .get(&origin)
        .and_then(|store| store.entries.get(&id))
        .filter(|entry| entry.kind == EntryKind::File)
        .map(|entry| entry.bytes.clone())
        .unwrap_or_default()
}

#[op2]
#[buffer]
pub fn op_opfs_read(#[string] origin: String, #[smi] id: u32) -> Vec<u8> {
    read_impl(origin, id)
}

fn size_impl(origin: String, id: u32) -> i32 {
    let guard = registry().lock().unwrap_or_else(|e| e.into_inner());
    guard
        .get(&origin)
        .and_then(|store| store.entries.get(&id))
        .filter(|entry| entry.kind == EntryKind::File)
        .map(|entry| i32::try_from(entry.bytes.len()).unwrap_or(i32::MAX))
        .unwrap_or(0)
}

#[op2(fast)]
pub fn op_opfs_size(#[string] origin: String, #[smi] id: u32) -> i32 {
    size_impl(origin, id)
}

fn write_impl(origin: String, id: u32, at: usize, data: &[u8]) -> i32 {
    let mut guard = registry().lock().unwrap_or_else(|e| e.into_inner());
    let Some(entry) = guard
        .get_mut(&origin)
        .and_then(|store| store.entries.get_mut(&id))
        .filter(|entry| entry.kind == EntryKind::File)
    else {
        return 0;
    };
    let required = at.saturating_add(data.len());
    if entry.bytes.len() < required {
        entry.bytes.resize(required, 0);
    }
    entry.bytes[at..at + data.len()].copy_from_slice(data);
    i32::try_from(data.len()).unwrap_or(i32::MAX)
}

#[op2(fast)]
pub fn op_opfs_write(
    #[string] origin: String,
    #[smi] id: u32,
    #[smi] at: u32,
    #[buffer] data: &[u8],
) -> i32 {
    write_impl(origin, id, at as usize, data)
}

fn truncate_impl(origin: String, id: u32, size: usize) -> bool {
    let mut guard = registry().lock().unwrap_or_else(|e| e.into_inner());
    let Some(entry) = guard
        .get_mut(&origin)
        .and_then(|store| store.entries.get_mut(&id))
        .filter(|entry| entry.kind == EntryKind::File)
    else {
        return false;
    };
    entry.bytes.resize(size, 0);
    true
}

#[op2(fast)]
pub fn op_opfs_truncate(#[string] origin: String, #[smi] id: u32, #[smi] size: u32) -> bool {
    truncate_impl(origin, id, size as usize)
}

fn move_impl(origin: String, id: u32, new_name: String) -> bool {
    let mut guard = registry().lock().unwrap_or_else(|e| e.into_inner());
    let Some(store) = guard.get_mut(&origin) else {
        return false;
    };
    let Some(parent_id) = store.entries.get(&id).and_then(|entry| entry.parent) else {
        return false;
    };
    if let Some(existing_id) = child_id(store, parent_id, &new_name) {
        return existing_id == id;
    }
    let Some(entry) = store.entries.get_mut(&id) else {
        return false;
    };
    entry.name = new_name;
    true
}

#[op2(fast)]
pub fn op_opfs_move(#[string] origin: String, #[smi] id: u32, #[string] new_name: String) -> bool {
    move_impl(origin, id, new_name)
}

deno_core::extension!(
    opfs_extension,
    ops = [
        op_opfs_root,
        op_opfs_stat,
        op_opfs_get_child,
        op_opfs_list,
        op_opfs_remove_child,
        op_opfs_resolve,
        op_opfs_read,
        op_opfs_size,
        op_opfs_write,
        op_opfs_truncate,
        op_opfs_move,
    ],
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_is_shared_by_origin_and_keeps_stable_entry_identity() {
        let origin = "https://opfs-host-a.example".to_string();
        let other = "https://opfs-host-b.example".to_string();
        let root = root_impl(origin.clone());
        let file = get_child_impl(
            origin.clone(),
            root.id,
            "shared.bin".into(),
            "file".into(),
            true,
        );
        assert_eq!(file.status, 1);
        assert_eq!(write_impl(origin.clone(), file.id, 0, &[7, 8, 9]), 3);
        assert_eq!(read_impl(origin.clone(), file.id), vec![7, 8, 9]);

        let reopened = get_child_impl(
            origin.clone(),
            root.id,
            "shared.bin".into(),
            "file".into(),
            false,
        );
        assert_eq!(reopened.id, file.id, "same entry must keep a stable id");
        assert!(move_impl(origin.clone(), file.id, "renamed.bin".into()));
        assert_eq!(stat_impl(origin.clone(), file.id).name, "renamed.bin");
        assert_eq!(read_impl(origin.clone(), file.id), vec![7, 8, 9]);

        let other_root = root_impl(other.clone());
        assert_eq!(
            get_child_impl(
                other,
                other_root.id,
                "renamed.bin".into(),
                "file".into(),
                false,
            )
            .status,
            0,
            "OPFS must remain origin isolated"
        );

        assert_eq!(
            remove_child_impl(origin, root.id, "renamed.bin".into(), false),
            1
        );
    }

    #[test]
    fn directory_listing_resolve_and_recursive_remove_preserve_tree_semantics() {
        let origin = "https://opfs-host-tree.example".to_string();
        let root = root_impl(origin.clone());
        let dir = get_child_impl(
            origin.clone(),
            root.id,
            "dir".into(),
            "directory".into(),
            true,
        );
        let file = get_child_impl(
            origin.clone(),
            dir.id,
            "file.txt".into(),
            "file".into(),
            true,
        );
        assert_eq!(
            resolve_impl(origin.clone(), root.id, file.id),
            Some(vec!["dir".into(), "file.txt".into()])
        );
        assert_eq!(
            remove_child_impl(origin.clone(), root.id, "dir".into(), false),
            -1
        );
        assert_eq!(
            remove_child_impl(origin.clone(), root.id, "dir".into(), true),
            1
        );
        assert!(list_impl(origin, root.id).is_empty());
    }
}
