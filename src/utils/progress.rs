use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};

pub(crate) fn spinner(msg: &str) -> ProgressBar {
    let pb = spinner_with_style(msg);
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}

/// Like [`spinner`], but without a self-ticking redraw thread.
///
/// Use this for operations where another process may print straight to the
/// terminal (e.g. `ssh` prompting for a key passphrase): indicatif's steady
/// tick redraws the spinner's line every interval, which clobbers whatever
/// that other process just wrote there.
pub(crate) fn static_spinner(msg: &str) -> ProgressBar {
    spinner_with_style(msg)
}

fn spinner_with_style(msg: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message(msg.to_string());
    pb
}
