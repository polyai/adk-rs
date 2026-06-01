use rich_rust::console::PrintOptions;
use rich_rust::renderables::{Panel, Traceback};
use rich_rust::{Console, Style, Theme};
use std::io;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing_subscriber::EnvFilter;

static STDOUT_CONSOLE: OnceLock<Console> = OnceLock::new();
static STDERR_CONSOLE: OnceLock<Console> = OnceLock::new();
static VERBOSE: AtomicBool = AtomicBool::new(false);
static DEBUG: AtomicBool = AtomicBool::new(false);

pub(crate) fn configure(verbose: bool, debug: bool) {
    set_verbose(verbose);
    DEBUG.store(debug, Ordering::Relaxed);

    let filter = if debug { "debug" } else { "warn" };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(filter))
        .with_writer(io::stderr)
        .try_init();
}

pub(crate) fn set_verbose(verbose: bool) {
    VERBOSE.store(verbose, Ordering::Relaxed);
}

pub(crate) fn verbose() -> bool {
    VERBOSE.load(Ordering::Relaxed)
}

pub(crate) fn success(message: impl AsRef<str>) {
    print_stdout(&format!("[success]{}[/success]", message.as_ref()));
}

pub(crate) fn error(message: impl AsRef<str>) {
    print_stderr(&format!("[error]Error:[/error] {}", message.as_ref()));
}

pub(crate) fn warning(message: impl AsRef<str>) {
    print_stdout(&format!("[warning]Warning:[/warning] {}", message.as_ref()));
}

pub(crate) fn info(message: impl AsRef<str>) {
    print_stdout(&format!("[info]{}[/info]", message.as_ref()));
}

pub(crate) fn plain(message: impl AsRef<str>) {
    print_stdout(message.as_ref());
}

pub(crate) fn plain_stderr(message: impl AsRef<str>) {
    print_stderr(message.as_ref());
}

pub(crate) fn prompt(message: impl AsRef<str>) -> io::Result<()> {
    print_stdout_with_end(message.as_ref(), "")
}

pub(crate) fn print_welcome_message() {
    plain("");
    let panel = Panel::from_text(POLY_LOGO)
        .style(poly_logo_style())
        .border_style(poly_logo_border_style())
        .padding((1, 6));
    stdout_console().print_renderable(&panel);
    plain("[label]Welcome to the PolyAI Agent Development Kit (ADK)![/label]");
    plain("Build and edit Agent Studio projects locally with the PolyAI ADK");
    plain("Documentation: https://polyai.github.io/adk/");
    plain("");
}

pub(crate) fn exception(message: impl AsRef<str>) {
    let message = message.as_ref();
    if verbose() {
        let traceback = Traceback::capture("Error", message);
        err_console().print_exception(&traceback);
    } else {
        error(message);
        print_stderr("[muted]Run with --verbose for the full traceback.[/muted]");
    }
}

const POLY_LOGO: &str = r#"        ●
    ●   ●   ●      ██████   ██████  ██   ██    ██   █████  ██
      ●   ●        ██   ██ ██    ██ ██    ██  ██   ██   ██ ██
    ●   ●   ●      ██████  ██    ██ ██     ████    ███████ ██
      ●   ●        ██      ██    ██ ██      ██     ██   ██ ██
    ●   ●   ●      ██       ██████  ██████  ██     ██   ██ ██
        ●"#;

fn print_stdout(message: &str) {
    let _ = print_stdout_with_end(message, "\n");
}

fn print_stdout_with_end(message: &str, end: &str) -> io::Result<()> {
    let mut stdout = io::stdout();
    stdout_console().print_to(&mut stdout, message, &options(end))
}

fn print_stderr(message: &str) {
    let mut stderr = io::stderr();
    let _ = err_console().print_to(&mut stderr, message, &options("\n"));
}

fn stdout_console() -> &'static Console {
    STDOUT_CONSOLE.get_or_init(|| {
        Console::builder()
            .theme(theme())
            .markup(true)
            .highlight(false)
            .build()
    })
}

fn err_console() -> &'static Console {
    STDERR_CONSOLE.get_or_init(|| {
        Console::builder()
            .theme(theme())
            .markup(true)
            .highlight(false)
            .file(Box::new(io::stderr()))
            .build()
    })
}

fn options(end: &str) -> PrintOptions {
    PrintOptions::new().with_markup(true).with_end(end)
}

fn poly_logo_style() -> Style {
    Style::parse("bold #D9EE50 on black").expect("valid PolyAI logo style")
}

fn poly_logo_border_style() -> Style {
    Style::parse("#D9EE50").expect("valid PolyAI logo border style")
}

fn theme() -> Theme {
    Theme::from_style_definitions(
        [
            ("info", "cyan"),
            ("success", "green"),
            ("warning", "yellow"),
            ("error", "red bold"),
            ("filename.new", "green"),
            ("filename.modified", "green"),
            ("filename.deleted", "red"),
            ("filename.conflict", "red bold"),
            ("label", "bold"),
            ("muted", "dim"),
        ],
        true,
    )
    .expect("valid ADK console theme")
}
