use std::time::{Duration, Instant};

use corvo_terminal::{Event, Frame, Half, Launch, Pick, Size, Terminal};
use futures::StreamExt as _;
use futures::channel::mpsc::UnboundedReceiver;

const WAIT: Duration = Duration::from_secs(10);

fn sh() -> (Terminal, UnboundedReceiver<Event>) {
    let launch = Launch {
        cwd: std::env::temp_dir(),
        program: Some(("sh".to_owned(), Vec::new())),
        size: Size::new(80, 24),
    };
    Terminal::spawn(launch).expect("sh starts")
}

// Waits until the screen shows what is looked for.
fn until(terminal: &Terminal, found: impl Fn(&Frame) -> bool) -> Frame {
    let start = Instant::now();
    loop {
        let frame = terminal.frame();
        if found(&frame) {
            return frame;
        }
        assert!(
            start.elapsed() < WAIT,
            "never showed up:\n{}",
            screen(&frame)
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn screen(frame: &Frame) -> String {
    (0..frame.rows)
        .map(|row| frame.text(row))
        .collect::<Vec<_>>()
        .join("\n")
}

fn shows(text: &'static str) -> impl Fn(&Frame) -> bool {
    move |frame| (0..frame.rows).any(|row| frame.text(row) == text)
}

#[test]
fn what_is_typed_runs_and_shows() {
    let (terminal, _events) = sh();

    terminal.type_line("echo corvo-$((40 + 2))", true);

    until(&terminal, shows("corvo-42"));
}

#[test]
fn a_line_left_typed_does_not_run() {
    let (terminal, _events) = sh();

    terminal.type_line("echo $((1 + 1))", false);
    until(&terminal, |frame| {
        (0..frame.rows).any(|row| frame.text(row).ends_with("echo $((1 + 1))"))
    });
    std::thread::sleep(Duration::from_millis(200));

    assert!(!shows("2")(&terminal.frame()));
}

#[test]
fn the_screen_takes_the_new_size() {
    let (terminal, _events) = sh();

    terminal.resize(Size::new(100, 30));
    terminal.type_line("stty size", true);

    let frame = until(&terminal, shows("30 100"));
    assert_eq!((frame.cols, frame.rows), (100, 30));
}

#[test]
fn what_runs_in_front_of_the_shell_is_named() {
    let (terminal, _events) = sh();
    until(&terminal, |frame| frame.cursor.is_some());
    assert_eq!(terminal.running(), None);

    terminal.type_line("sleep 5", true);

    let start = Instant::now();
    while terminal.running().as_deref() != Some("sleep") {
        assert!(start.elapsed() < WAIT, "sleep never ran in front");
        std::thread::sleep(Duration::from_millis(20));
    }
    terminal.input(vec![3]);
    let start = Instant::now();
    while terminal.running().is_some() {
        assert!(start.elapsed() < WAIT, "ctrl-c did not stop sleep");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn the_end_of_the_shell_comes_with_its_code() {
    let (terminal, mut events) = sh();

    terminal.type_line("exit 3", true);

    let ended = futures::executor::block_on(async {
        while let Some(event) = events.next().await {
            if let Event::Exited(code) = event {
                return Some(code);
            }
        }
        None
    });
    assert_eq!(ended, Some(Some(3)));
    assert!(terminal.has_exited());
}

#[test]
fn colors_reach_the_cells() {
    let (terminal, _events) = sh();

    terminal.type_line("printf '\\033[31mred\\033[0m\\n'", true);

    let frame = until(&terminal, shows("red"));
    let row = (0..frame.rows)
        .find(|row| frame.text(*row) == "red")
        .unwrap();
    assert_eq!(frame.lines[row][0].fg, corvo_terminal::palette(1));
    assert_eq!(frame.lines[row][0].bg, None);
}

#[test]
fn text_picked_on_the_screen_can_be_copied() {
    let (terminal, _events) = sh();
    terminal.type_line("echo picked words", true);
    let frame = until(&terminal, shows("picked words"));
    let row = (0..frame.rows)
        .find(|row| frame.text(*row) == "picked words")
        .unwrap();

    terminal.select(row, 0, Half::Left, Pick::Cells);
    terminal.extend(row, 5, Half::Right);
    assert_eq!(terminal.selected().as_deref(), Some("picked"));

    terminal.select(row, 9, Half::Left, Pick::Word);
    assert_eq!(terminal.selected().as_deref(), Some("words"));

    terminal.select(row, 3, Half::Left, Pick::Cells);
    assert_eq!(terminal.selected(), None);
}

#[test]
fn a_line_typed_while_the_shell_starts_shows_once() {
    let (terminal, _events) = sh();

    terminal.type_line("echo once-$((1))", true);

    let frame = until(&terminal, shows("once-1"));
    let typed = (0..frame.rows)
        .filter(|row| frame.text(*row).contains("echo once-"))
        .count();
    assert_eq!(typed, 1, "\n{}", screen(&frame));
}

#[test]
fn a_script_run_by_a_runtime_is_named_after_the_script() {
    let dir = std::env::temp_dir().join(format!("corvo-term-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("serve.sh");
    std::fs::write(&script, "sleep 5\n").unwrap();
    let (terminal, _events) = sh();
    until(&terminal, |frame| frame.cursor.is_some());

    terminal.type_line(&format!("sh {}", script.display()), true);

    let start = Instant::now();
    while terminal.running().as_deref() != Some("serve.sh") {
        assert!(
            start.elapsed() < WAIT,
            "named {:?} instead",
            terminal.running()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    terminal.input(vec![3]);
    std::fs::remove_dir_all(&dir).ok();
}
