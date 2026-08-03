//! JS↔Rust DOM boundary microbenchmark: builds one moderate DOM, then times the
//! hot per-call accessors (matches/closest/contains/traversal/nodeType) from
//! Rust. These do their work per call, so the wrapper cache does not hide the
//! boundary cost.
//!
//!   cargo run --release --example dom_bench -p browser_oxide

use browser_oxide::stealth::presets::chrome_148_windows;
use browser_oxide::Page;
use std::time::Instant;

fn bench(page: &mut Page, label: &str, iters: u64, js: &str) {
    for _ in 0..3 {
        let _ = page.evaluate(js);
    }
    let mut times = Vec::with_capacity(9);
    for _ in 0..9 {
        let t = Instant::now();
        let r = page.evaluate(js);
        times.push((t.elapsed(), r));
    }
    times.sort_by_key(|(d, _)| *d);
    let med = times[times.len() / 2].0;
    let per = med.as_nanos() as f64 / iters as f64;
    let sample = times[0].1.as_deref().unwrap_or("<err>").to_string();
    println!("{label:<24} median={med:>10.3?}  per-call={per:>8.1}ns  (iters={iters}, r={sample})");
}

const BUILD: &str = r#"(function(){
  var body = document.body;
  var app = document.createElement('div'); app.id='app'; app.className='app'; body.appendChild(app);
  for (var s=0;s<10;s++){
    var sec=document.createElement('section'); sec.className='sec'; app.appendChild(sec);
    for (var r=0;r<20;r++){
      var row=document.createElement('div'); row.className='row'+(r%2?' active':''); sec.appendChild(row);
      var cell=document.createElement('span'); cell.className='cell'; cell.textContent='c'+r; row.appendChild(cell);
    }
  }
  globalThis.__app = app;
  globalThis.__deepRow = document.querySelector('.sec .row.active');
  globalThis.__deepCell = document.querySelector('.sec .row.active .cell');
  globalThis.__sec = document.querySelector('.sec');
  return 'built '+document.querySelectorAll('*').length+' els';
})()"#;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let mut page = Page::from_html_fast(
                "<html><head></head><body></body></html>",
                "about:blank",
                chrome_148_windows(),
            )
            .await
            .unwrap();

            println!("\n=== DOM boundary microbench ===");
            println!("build: {}", page.evaluate(BUILD).unwrap_or_default());

            bench(&mut page, "matches", 50_000,
                "(function(){var e=globalThis.__deepRow,n=0;for(var k=0;k<50000;k++){if(e.matches('div.row.active'))n++;}return n;})()");
            bench(&mut page, "closest", 20_000,
                "(function(){var e=globalThis.__deepCell,n=0;for(var k=0;k<20000;k++){if(e.closest('#app'))n++;}return n;})()");
            bench(&mut page, "contains", 50_000,
                "(function(){var a=globalThis.__app,e=globalThis.__deepCell,n=0;for(var k=0;k<50000;k++){if(a.contains(e))n++;}return n;})()");
            bench(&mut page, "isConnected", 50_000,
                "(function(){var e=globalThis.__deepCell,n=0;for(var k=0;k<50000;k++){if(e.isConnected)n++;}return n;})()");
            bench(&mut page, "nextElementSibling walk", 5_000,
                "(function(){var sec=globalThis.__sec,n=0;for(var k=0;k<5000;k++){var c=sec.firstElementChild;while(c){n++;c=c.nextElementSibling;}}return n;})()");
            bench(&mut page, "nodeType access", 200_000,
                "(function(){var e=globalThis.__deepRow,n=0;for(var k=0;k<200000;k++){if(e.nodeType===1)n++;}return n;})()");
            bench(&mut page, "tagName access", 100_000,
                "(function(){var e=globalThis.__deepRow,n=0;for(var k=0;k<100000;k++){if(e.tagName==='DIV')n++;}return n;})()");
            bench(&mut page, "getElementById", 20_000,
                "(function(){var n=0;for(var k=0;k<20000;k++){if(document.getElementById('app'))n++;}return n;})()");
            bench(&mut page, "firstElementChild+count", 50_000,
                "(function(){var sec=globalThis.__sec,n=0;for(var k=0;k<50000;k++){if(sec.firstElementChild)n++;n+=sec.childElementCount;}return n;})()");

            // appendChild of a 100-node script-free subtree: exercises the
            // _onNodeInserted insertion scan.
            page.evaluate("(function(){var t=document.createElement('div');for(var i=0;i<100;i++){var d=document.createElement('div');d.className='n';t.appendChild(d);}globalThis.__tree=t;})()").ok();
            bench(&mut page, "appendChild 100-subtree", 5_000,
                "(function(){var b=document.body,t=globalThis.__tree,n=0;for(var k=0;k<5000;k++){b.appendChild(t);b.removeChild(t);n++;}return n;})()");
        })
        .await;
}
