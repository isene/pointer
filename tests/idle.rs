//! An idle pointer costs nothing, and still sees a new file at once.
#![cfg(target_os = "linux")]

use std::time::Duration;

#[test]
fn it_waits_without_waking_up_and_shows_a_new_file() {
    let home = std::env::temp_dir().join(format!("pointer-test-{}", std::process::id()));
    let dir = home.join("work");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("first.txt"), "x").unwrap();
    let mut app = still::Run::new(env!("CARGO_BIN_EXE_pointer")).arg(&dir).env("HOME", &home).size(120, 40).start().unwrap();
    app.settle(Duration::from_millis(800));

    let idle = app.idle(Duration::from_secs(3)).unwrap();
    assert!(idle.is_still(), "{idle:?}");

    std::fs::write(dir.join("second.txt"), "x").unwrap();
    let out = String::from_utf8_lossy(&app.output(Duration::from_millis(500))).into_owned();
    assert!(out.contains("second.txt"), "the new file was not drawn");

    app.hang_up();
    assert!(app.ended(Duration::from_secs(1)));
    let _ = std::fs::remove_dir_all(&home);
}
