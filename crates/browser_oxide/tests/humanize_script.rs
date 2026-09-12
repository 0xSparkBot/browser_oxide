#[test]
fn repeated_humanization_is_background_and_stops_after_load() {
    let source = include_str!("../src/js/humanize.js");

    assert!(
        source.contains("if (document.readyState === 'complete') return;"),
        "repeated synthetic input must stop after the document completes"
    );
    assert!(
        source.contains("_sched(function repeatCycle()"),
        "repeat scheduling must use the engine background-timer helper"
    );
    assert!(
        !source.contains("setInterval(runCycle, 4000)"),
        "a permanent humanize interval can re-activate settled page work forever"
    );
}
