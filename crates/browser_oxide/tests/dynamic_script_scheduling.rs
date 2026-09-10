use browser_oxide::Page;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

fn spawn_script_server(
    routes: Vec<(&'static str, Duration, &'static str)>,
) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        let mut workers = Vec::new();
        for _ in 0..routes.len() {
            let (mut socket, _) = listener.accept().unwrap();
            let routes = routes.clone();
            workers.push(std::thread::spawn(move || {
                let mut request = [0_u8; 4096];
                let n = socket.read(&mut request).unwrap_or(0);
                let request = String::from_utf8_lossy(&request[..n]);
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/")
                    .split('?')
                    .next()
                    .unwrap_or("/");
                let (_, delay, source) = routes
                    .iter()
                    .find(|(candidate, _, _)| *candidate == path)
                    .expect("known script path");
                std::thread::sleep(*delay);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/javascript\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    source.len(),
                    source,
                );
                socket.write_all(response.as_bytes()).unwrap();
            }));
        }
        for worker in workers {
            worker.join().unwrap();
        }
    });
    (format!("http://{address}"), handle)
}

async fn empty_page(base: &str) -> Page {
    Page::from_html_with_url(
        "<!doctype html><html><head></head><body></body></html>",
        &format!("{base}/page"),
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn script_created_external_script_starts_force_async() {
    let mut page = Page::from_html(
        "<!doctype html><html><head></head><body></body></html>",
        Some(browser_oxide::stealth::presets::chrome_148_macos()),
    )
    .await
    .unwrap();

    assert_eq!(
        page.evaluate(
            r#"(() => {
                const s = document.createElement('script');
                const initial = [s.async, s.hasAttribute('async')];
                s.async = false;
                const afterFalse = [s.async, s.hasAttribute('async')];
                s.async = true;
                const afterTrue = [s.async, s.hasAttribute('async')];
                return JSON.stringify({initial, afterFalse, afterTrue});
            })()"#,
        )
        .unwrap(),
        r#"{"initial":[true,false],"afterFalse":[false,false],"afterTrue":[true,true]}"#
    );
}

#[tokio::test(flavor = "current_thread")]
async fn default_async_external_script_insertion_does_not_block_on_network() {
    let (base, server) = spawn_script_server(vec![(
        "/slow.js",
        Duration::from_millis(250),
        "globalThis.__dynamicLoaded = true;",
    )]);
    let mut page = empty_page(&base).await;

    let started = Instant::now();
    page.evaluate(&format!(
        r#"globalThis.__dynamicLoaded = false;
        const s = document.createElement('script');
        s.src = {base:?} + '/slow.js';
        document.head.appendChild(s);"#,
    ))
    .unwrap();
    assert!(
        started.elapsed() < Duration::from_millis(100),
        "script-created default-async insertion synchronously waited for network: {:?}",
        started.elapsed()
    );

    page.evaluate_async(
        "new Promise(resolve => setTimeout(resolve, 350))",
        Duration::from_secs(1),
    )
    .await
    .unwrap();
    assert_eq!(
        page.evaluate("String(globalThis.__dynamicLoaded)").unwrap(),
        "true"
    );
    server.join().unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn explicit_async_false_fetches_without_blocking_but_executes_in_insertion_order() {
    let (base, server) = spawn_script_server(vec![
        (
            "/slow.js",
            Duration::from_millis(180),
            "globalThis.__dynamicOrder.push('slow');",
        ),
        (
            "/fast.js",
            Duration::from_millis(10),
            "globalThis.__dynamicOrder.push('fast');",
        ),
    ]);
    let mut page = empty_page(&base).await;

    let started = Instant::now();
    page.evaluate(&format!(
        r#"globalThis.__dynamicOrder = [];
        const slow = document.createElement('script');
        slow.async = false;
        slow.src = {base:?} + '/slow.js';
        document.head.appendChild(slow);
        const fast = document.createElement('script');
        fast.async = false;
        fast.src = {base:?} + '/fast.js';
        document.head.appendChild(fast);"#,
    ))
    .unwrap();
    assert!(
        started.elapsed() < Duration::from_millis(100),
        "async=false insertion must not synchronously fetch: {:?}",
        started.elapsed()
    );

    page.evaluate_async(
        "new Promise(resolve => setTimeout(resolve, 350))",
        Duration::from_secs(1),
    )
    .await
    .unwrap();
    assert_eq!(
        page.evaluate("JSON.stringify(globalThis.__dynamicOrder)")
            .unwrap(),
        r#"["slow","fast"]"#
    );
    server.join().unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn default_async_scripts_execute_in_fetch_completion_order() {
    let (base, server) = spawn_script_server(vec![
        (
            "/slow.js",
            Duration::from_millis(180),
            "globalThis.__dynamicOrder.push('slow');",
        ),
        (
            "/fast.js",
            Duration::from_millis(10),
            "globalThis.__dynamicOrder.push('fast');",
        ),
    ]);
    let mut page = empty_page(&base).await;

    page.evaluate(&format!(
        r#"globalThis.__dynamicOrder = [];
        for (const name of ['slow', 'fast']) {{
            const script = document.createElement('script');
            script.src = {base:?} + '/' + name + '.js';
            document.head.appendChild(script);
        }}"#,
    ))
    .unwrap();

    page.evaluate_async(
        "new Promise(resolve => setTimeout(resolve, 350))",
        Duration::from_secs(1),
    )
    .await
    .unwrap();
    assert_eq!(
        page.evaluate("JSON.stringify(globalThis.__dynamicOrder)")
            .unwrap(),
        r#"["fast","slow"]"#
    );
    server.join().unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn dynamic_classic_load_event_is_trusted_once_and_current_script_is_correct() {
    let (base, server) = spawn_script_server(vec![(
        "/identity.js",
        Duration::from_millis(10),
        "globalThis.__currentScriptMatches = document.currentScript && document.currentScript.id === 'dynamic-entry';",
    )]);
    let mut page = empty_page(&base).await;

    page.evaluate_async(
        &format!(
            r#"new Promise((resolve, reject) => {{
                globalThis.__dynamicLoadEvents = [];
                const script = document.createElement('script');
                script.id = 'dynamic-entry';
                script.src = {base:?} + '/identity.js';
                script.addEventListener('load', event => {{
                    globalThis.__dynamicLoadEvents.push({{trusted: event.isTrusted}});
                    resolve();
                }});
                script.addEventListener('error', () => reject(new Error('dynamic script failed')));
                document.head.appendChild(script);
            }})"#,
        ),
        Duration::from_secs(2),
    )
    .await
    .unwrap();

    assert_eq!(
        page.evaluate(
            "JSON.stringify({matches: !!globalThis.__currentScriptMatches, events: globalThis.__dynamicLoadEvents})",
        )
        .unwrap(),
        r#"{"matches":true,"events":[{"trusted":true}]}"#
    );
    server.join().unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn dynamic_external_module_executes_module_graph_and_has_null_current_script() {
    let (base, server) = spawn_script_server(vec![
        (
            "/entry.mjs",
            Duration::from_millis(10),
            "import { value } from './dep.mjs'; globalThis.__dynamicModule = { value, currentScriptNull: document.currentScript === null };",
        ),
        (
            "/dep.mjs",
            Duration::from_millis(5),
            "export const value = 42;",
        ),
    ]);
    let mut page = empty_page(&base).await;

    page.evaluate_async(
        &format!(
            r#"new Promise((resolve, reject) => {{
                const script = document.createElement('script');
                script.type = 'module';
                script.src = {base:?} + '/entry.mjs';
                script.addEventListener('load', event => resolve(event.isTrusted));
                script.addEventListener('error', () => reject(new Error('dynamic module failed')));
                document.head.appendChild(script);
            }}).then(trusted => {{ globalThis.__dynamicModuleLoadTrusted = trusted; }})"#,
        ),
        Duration::from_secs(2),
    )
    .await
    .unwrap();

    assert_eq!(
        page.evaluate(
            "JSON.stringify({module: globalThis.__dynamicModule, trusted: globalThis.__dynamicModuleLoadTrusted})",
        )
        .unwrap(),
        r#"{"module":{"value":42,"currentScriptNull":true},"trusted":true}"#
    );
    server.join().unwrap();
}
