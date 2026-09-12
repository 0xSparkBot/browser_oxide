//! Process-wide backing store for the CacheStorage Web API.
//!
//! CacheStorage is origin-scoped and shared by Window/Worker execution
//! contexts. A JS-only Map inside each V8 isolate cannot satisfy that model,
//! so the WebIDL layer serializes Request/Response snapshots into this small
//! host registry. Persistence is intentionally process-lifetime only.

use deno_core::op2;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CacheRecordSnapshot {
    pub url: String,
    pub method: String,
    pub request_headers: Vec<(String, String)>,
    pub status: i32,
    pub status_text: String,
    pub response_url: String,
    pub response_headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

#[derive(Default)]
struct CacheBucket {
    name: String,
    records: Vec<CacheRecordSnapshot>,
}

#[derive(Default)]
struct OriginStore {
    // Vec preserves the insertion order exposed by CacheStorage.keys().
    caches: Vec<CacheBucket>,
}

fn registry() -> &'static Mutex<HashMap<String, OriginStore>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, OriginStore>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cache_index(store: &OriginStore, name: &str) -> Option<usize> {
    store.caches.iter().position(|cache| cache.name == name)
}

fn ensure_cache<'a>(store: &'a mut OriginStore, name: &str) -> &'a mut CacheBucket {
    if let Some(index) = cache_index(store, name) {
        return &mut store.caches[index];
    }
    store.caches.push(CacheBucket {
        name: name.to_string(),
        records: Vec::new(),
    });
    store.caches.last_mut().expect("just pushed cache bucket")
}

fn cache_storage_open_impl(origin: String, cache_name: String) {
    let mut guard = registry().lock().unwrap_or_else(|e| e.into_inner());
    let store = guard.entry(origin).or_default();
    let _ = ensure_cache(store, &cache_name);
}

#[op2(fast)]
pub fn op_cache_storage_open(#[string] origin: String, #[string] cache_name: String) {
    cache_storage_open_impl(origin, cache_name);
}

fn cache_storage_has_impl(origin: String, cache_name: String) -> bool {
    let guard = registry().lock().unwrap_or_else(|e| e.into_inner());
    guard
        .get(&origin)
        .is_some_and(|store| cache_index(store, &cache_name).is_some())
}

#[op2(fast)]
pub fn op_cache_storage_has(#[string] origin: String, #[string] cache_name: String) -> bool {
    cache_storage_has_impl(origin, cache_name)
}

fn cache_storage_names_impl(origin: String) -> Vec<String> {
    let guard = registry().lock().unwrap_or_else(|e| e.into_inner());
    guard
        .get(&origin)
        .map(|store| {
            store
                .caches
                .iter()
                .map(|cache| cache.name.clone())
                .collect()
        })
        .unwrap_or_default()
}

#[op2]
#[serde]
pub fn op_cache_storage_names(#[string] origin: String) -> Vec<String> {
    cache_storage_names_impl(origin)
}

fn cache_storage_delete_impl(origin: String, cache_name: String) -> bool {
    let mut guard = registry().lock().unwrap_or_else(|e| e.into_inner());
    let Some(store) = guard.get_mut(&origin) else {
        return false;
    };
    let Some(index) = cache_index(store, &cache_name) else {
        return false;
    };
    store.caches.remove(index);
    if store.caches.is_empty() {
        guard.remove(&origin);
    }
    true
}

#[op2(fast)]
pub fn op_cache_storage_delete(#[string] origin: String, #[string] cache_name: String) -> bool {
    cache_storage_delete_impl(origin, cache_name)
}

fn cache_storage_records_impl(origin: String, cache_name: String) -> Vec<CacheRecordSnapshot> {
    let guard = registry().lock().unwrap_or_else(|e| e.into_inner());
    guard
        .get(&origin)
        .and_then(|store| cache_index(store, &cache_name).map(|index| &store.caches[index]))
        .map(|cache| cache.records.clone())
        .unwrap_or_default()
}

#[op2]
#[serde]
pub fn op_cache_storage_records(
    #[string] origin: String,
    #[string] cache_name: String,
) -> Vec<CacheRecordSnapshot> {
    cache_storage_records_impl(origin, cache_name)
}

#[allow(
    clippy::too_many_arguments,
    reason = "mirrors the serialized CacheStorage Request/Response snapshot fields passed by the op"
)]
fn cache_storage_put_impl(
    origin: String,
    cache_name: String,
    url: String,
    method: String,
    request_headers: Vec<(String, String)>,
    status: i32,
    status_text: String,
    response_url: String,
    response_headers: Vec<(String, String)>,
    body: &[u8],
) {
    let mut guard = registry().lock().unwrap_or_else(|e| e.into_inner());
    let store = guard.entry(origin).or_default();
    let cache = ensure_cache(store, &cache_name);
    let snapshot = CacheRecordSnapshot {
        url: url.clone(),
        method,
        request_headers,
        status,
        status_text,
        response_url,
        response_headers,
        body: body.to_vec(),
    };
    if let Some(index) = cache.records.iter().position(|record| record.url == url) {
        cache.records[index] = snapshot;
    } else {
        cache.records.push(snapshot);
    }
}

#[op2]
pub fn op_cache_storage_put(
    #[string] origin: String,
    #[string] cache_name: String,
    #[string] url: String,
    #[string] method: String,
    #[serde] request_headers: Vec<(String, String)>,
    #[smi] status: i32,
    #[string] status_text: String,
    #[string] response_url: String,
    #[serde] response_headers: Vec<(String, String)>,
    #[buffer] body: &[u8],
) {
    cache_storage_put_impl(
        origin,
        cache_name,
        url,
        method,
        request_headers,
        status,
        status_text,
        response_url,
        response_headers,
        body,
    );
}

fn cache_storage_delete_record_impl(origin: String, cache_name: String, url: String) -> bool {
    let mut guard = registry().lock().unwrap_or_else(|e| e.into_inner());
    let Some(store) = guard.get_mut(&origin) else {
        return false;
    };
    let Some(cache_idx) = cache_index(store, &cache_name) else {
        return false;
    };
    let cache = &mut store.caches[cache_idx];
    let Some(record_idx) = cache.records.iter().position(|record| record.url == url) else {
        return false;
    };
    cache.records.remove(record_idx);
    true
}

#[op2(fast)]
pub fn op_cache_storage_delete_record(
    #[string] origin: String,
    #[string] cache_name: String,
    #[string] url: String,
) -> bool {
    cache_storage_delete_record_impl(origin, cache_name, url)
}

deno_core::extension!(
    cache_storage_extension,
    ops = [
        op_cache_storage_open,
        op_cache_storage_has,
        op_cache_storage_names,
        op_cache_storage_delete,
        op_cache_storage_records,
        op_cache_storage_put,
        op_cache_storage_delete_record,
    ],
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_preserves_order_and_origin_isolation() {
        let a = "https://cache-ext-a.example".to_string();
        let b = "https://cache-ext-b.example".to_string();
        cache_storage_open_impl(a.clone(), "first".into());
        cache_storage_open_impl(a.clone(), "second".into());
        cache_storage_open_impl(b.clone(), "first".into());
        assert_eq!(cache_storage_names_impl(a.clone()), vec!["first", "second"]);
        assert_eq!(cache_storage_names_impl(b.clone()), vec!["first"]);

        cache_storage_put_impl(
            a.clone(),
            "first".into(),
            "https://cache-ext-a.example/a".into(),
            "GET".into(),
            vec![],
            200,
            "OK".into(),
            "".into(),
            vec![],
            b"a",
        );
        cache_storage_put_impl(
            a.clone(),
            "first".into(),
            "https://cache-ext-a.example/b".into(),
            "GET".into(),
            vec![],
            200,
            "OK".into(),
            "".into(),
            vec![],
            b"b",
        );
        let records = cache_storage_records_impl(a.clone(), "first".into());
        assert_eq!(
            records
                .iter()
                .map(|record| record.url.as_str())
                .collect::<Vec<_>>(),
            vec![
                "https://cache-ext-a.example/a",
                "https://cache-ext-a.example/b"
            ]
        );
        assert!(cache_storage_records_impl(b.clone(), "first".into()).is_empty());

        // Replacing an entry keeps its insertion position.
        cache_storage_put_impl(
            a.clone(),
            "first".into(),
            "https://cache-ext-a.example/a".into(),
            "GET".into(),
            vec![],
            201,
            "Created".into(),
            "".into(),
            vec![],
            b"new-a",
        );
        let records = cache_storage_records_impl(a.clone(), "first".into());
        assert_eq!(records[0].status, 201);
        assert_eq!(records[0].body, b"new-a");

        assert!(cache_storage_delete_impl(a.clone(), "first".into()));
        assert!(cache_storage_delete_impl(a, "second".into()));
        assert!(cache_storage_delete_impl(b, "first".into()));
    }
}
