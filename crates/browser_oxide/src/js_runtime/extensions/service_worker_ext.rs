//! Process-wide registration state for the Service Worker container API.
//!
//! This module intentionally models registration/query lifecycle only. It does
//! not execute Service Worker scripts or intercept fetches. Registrations are
//! origin-scoped and shared across Window runtimes, matching the observable
//! state of `register()`, `getRegistration(s)()`, `ready`, `update()` and
//! `unregister()` without pretending that a worker global is running.

use deno_core::op2;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceWorkerRegistrationSnapshot {
    pub id: u32,
    pub scope: String,
    pub script_url: String,
    pub update_via_cache: String,
}

#[derive(Clone, Debug)]
struct Registration {
    snapshot: ServiceWorkerRegistrationSnapshot,
    sequence: u64,
}

#[derive(Debug, Default)]
struct OriginStore {
    registrations: Vec<Registration>,
    next_id: u32,
    next_sequence: u64,
}

fn registry() -> &'static Mutex<HashMap<String, OriginStore>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, OriginStore>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn register_impl(
    origin: String,
    scope: String,
    script_url: String,
    update_via_cache: String,
) -> ServiceWorkerRegistrationSnapshot {
    let mut guard = registry().lock().unwrap_or_else(|e| e.into_inner());
    let store = guard.entry(origin).or_insert_with(|| OriginStore {
        registrations: Vec::new(),
        next_id: 1,
        next_sequence: 1,
    });

    if let Some(existing) = store
        .registrations
        .iter_mut()
        .find(|registration| registration.snapshot.scope == scope)
    {
        existing.snapshot.script_url = script_url;
        existing.snapshot.update_via_cache = update_via_cache;
        return existing.snapshot.clone();
    }

    let id = store.next_id;
    store.next_id = store.next_id.saturating_add(1);
    let sequence = store.next_sequence;
    store.next_sequence = store.next_sequence.saturating_add(1);
    let snapshot = ServiceWorkerRegistrationSnapshot {
        id,
        scope,
        script_url,
        update_via_cache,
    };
    store.registrations.push(Registration {
        snapshot: snapshot.clone(),
        sequence,
    });
    snapshot
}

#[op2]
#[serde]
pub fn op_service_worker_register(
    #[string] origin: String,
    #[string] scope: String,
    #[string] script_url: String,
    #[string] update_via_cache: String,
) -> ServiceWorkerRegistrationSnapshot {
    register_impl(origin, scope, script_url, update_via_cache)
}

fn list_impl(origin: String) -> Vec<ServiceWorkerRegistrationSnapshot> {
    let guard = registry().lock().unwrap_or_else(|e| e.into_inner());
    let Some(store) = guard.get(&origin) else {
        return Vec::new();
    };
    let mut registrations = store.registrations.iter().collect::<Vec<_>>();
    registrations.sort_by_key(|registration| registration.sequence);
    registrations
        .into_iter()
        .map(|registration| registration.snapshot.clone())
        .collect()
}

#[op2]
#[serde]
pub fn op_service_worker_list(#[string] origin: String) -> Vec<ServiceWorkerRegistrationSnapshot> {
    list_impl(origin)
}

fn match_impl(origin: String, client_url: String) -> Option<ServiceWorkerRegistrationSnapshot> {
    let guard = registry().lock().unwrap_or_else(|e| e.into_inner());
    let store = guard.get(&origin)?;
    store
        .registrations
        .iter()
        .filter(|registration| client_url.starts_with(&registration.snapshot.scope))
        .max_by_key(|registration| registration.snapshot.scope.len())
        .map(|registration| registration.snapshot.clone())
}

#[op2]
#[serde]
pub fn op_service_worker_match(
    #[string] origin: String,
    #[string] client_url: String,
) -> Option<ServiceWorkerRegistrationSnapshot> {
    match_impl(origin, client_url)
}

fn get_impl(origin: String, id: u32) -> Option<ServiceWorkerRegistrationSnapshot> {
    let guard = registry().lock().unwrap_or_else(|e| e.into_inner());
    guard
        .get(&origin)
        .and_then(|store| {
            store
                .registrations
                .iter()
                .find(|registration| registration.snapshot.id == id)
        })
        .map(|registration| registration.snapshot.clone())
}

#[op2]
#[serde]
pub fn op_service_worker_get(
    #[string] origin: String,
    #[smi] id: u32,
) -> Option<ServiceWorkerRegistrationSnapshot> {
    get_impl(origin, id)
}

fn unregister_impl(origin: String, id: u32) -> bool {
    let mut guard = registry().lock().unwrap_or_else(|e| e.into_inner());
    let Some(store) = guard.get_mut(&origin) else {
        return false;
    };
    let Some(index) = store
        .registrations
        .iter()
        .position(|registration| registration.snapshot.id == id)
    else {
        return false;
    };
    store.registrations.remove(index);
    if store.registrations.is_empty() {
        guard.remove(&origin);
    }
    true
}

#[op2(fast)]
pub fn op_service_worker_unregister(#[string] origin: String, #[smi] id: u32) -> bool {
    unregister_impl(origin, id)
}

deno_core::extension!(
    service_worker_extension,
    ops = [
        op_service_worker_register,
        op_service_worker_list,
        op_service_worker_match,
        op_service_worker_get,
        op_service_worker_unregister,
    ],
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registration_identity_scope_matching_and_origin_isolation() {
        let a = "https://sw-a.example".to_string();
        let b = "https://sw-b.example".to_string();
        let first = register_impl(
            a.clone(),
            "https://sw-a.example/app/".into(),
            "https://sw-a.example/sw.js".into(),
            "imports".into(),
        );
        let nested = register_impl(
            a.clone(),
            "https://sw-a.example/app/deep/".into(),
            "https://sw-a.example/deep-sw.js".into(),
            "none".into(),
        );
        assert_eq!(
            match_impl(a.clone(), "https://sw-a.example/app/page".into())
                .unwrap()
                .id,
            first.id
        );
        assert_eq!(
            match_impl(a.clone(), "https://sw-a.example/app/deep/page".into())
                .unwrap()
                .id,
            nested.id,
            "longest matching scope wins"
        );

        let updated = register_impl(
            a.clone(),
            first.scope.clone(),
            "https://sw-a.example/sw-v2.js".into(),
            "all".into(),
        );
        assert_eq!(
            updated.id, first.id,
            "same scope keeps registration identity"
        );
        assert_eq!(updated.script_url, "https://sw-a.example/sw-v2.js");
        assert_eq!(list_impl(a.clone()).len(), 2);
        assert!(list_impl(b.clone()).is_empty());
        assert!(match_impl(b, "https://sw-b.example/app/page".into()).is_none());

        assert!(unregister_impl(a.clone(), first.id));
        assert!(get_impl(a.clone(), first.id).is_none());
        assert!(unregister_impl(a, nested.id));
    }
}
