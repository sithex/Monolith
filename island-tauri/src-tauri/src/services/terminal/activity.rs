//! "Hermes is working" from the terminal title, never from chat content or output timing.
//!
//! Hermes' TUI marks its title with `⏳` while a turn runs, `✓` when idle and `⚠` while it
//! waits on the user (approval, sudo, secret or clarify prompt) (hermes-agent `ui-tui/src/app/useMainApp.ts`, sent as OSC 0/1/2 on every
//! change). Only those escape sequences are parsed, incrementally, so a title split across
//! reads still counts and text in the transcript never does.
//!
//! Output timing was tried first and is wrong: an idle Hermes TUI keeps redrawing its status
//! line (~500 B/s), which looked like work, so the notch pulsed all the time.

/// Longest title kept; anything longer is skipped rather than buffered without bound.
pub const MAX_TITLE: usize = 512;
/// Title marker Hermes uses while a turn is running.
pub const BUSY_MARKER: char = '⏳';
/// Title marker Hermes uses while it waits on the user (approval, sudo, secret, clarify).
pub const WAIT_MARKER: char = '⚠';

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Parse {
    #[default]
    Ground,
    Esc,
    /// Inside `ESC ]`, reading the numeric parameter.
    Param(u16),
    /// Collecting the text of an OSC 0/1/2 title.
    Title,
    /// `ESC` inside a title (possible `ESC \` terminator).
    TitleEsc,
    /// Another OSC, or an over-long title: skip to its terminator.
    Skip,
    SkipEsc,
}

#[derive(Debug, Default)]
pub struct TitleActivity {
    state: Parse,
    title: Vec<u8>,
    busy: bool,
    waiting: bool,
}

impl TitleActivity {
    /// Feed raw PTY output bytes.
    pub fn output(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.state = match (self.state, b) {
                (Parse::Ground, 0x1b) => Parse::Esc,
                (Parse::Ground, _) => Parse::Ground,
                (Parse::Esc, b']') => Parse::Param(0),
                (Parse::Esc, 0x1b) => Parse::Esc,
                (Parse::Esc, _) => Parse::Ground,
                (Parse::Param(n), b'0'..=b'9') if n < 1000 => {
                    Parse::Param(n * 10 + u16::from(b - b'0'))
                }
                (Parse::Param(n), b';') if n <= 2 => {
                    self.title.clear();
                    Parse::Title
                }
                (Parse::Param(_), 0x07) => Parse::Ground,
                (Parse::Param(_), _) => Parse::Skip,
                (Parse::Title, 0x07) => {
                    self.finish();
                    Parse::Ground
                }
                (Parse::Title, 0x1b) => Parse::TitleEsc,
                (Parse::Title, _) if self.title.len() >= MAX_TITLE => Parse::Skip,
                (Parse::Title, _) => {
                    self.title.push(b);
                    Parse::Title
                }
                (Parse::TitleEsc, b'\\') => {
                    self.finish();
                    Parse::Ground
                }
                (Parse::TitleEsc, b']') | (Parse::SkipEsc, b']') => Parse::Param(0),
                (Parse::TitleEsc, _) => Parse::Ground,
                (Parse::Skip, 0x07) => Parse::Ground,
                (Parse::Skip, 0x1b) => Parse::SkipEsc,
                (Parse::Skip, _) => Parse::Skip,
                (Parse::SkipEsc, b'\\') => Parse::Ground,
                (Parse::SkipEsc, _) => Parse::Skip,
            };
        }
    }

    fn finish(&mut self) {
        let title = String::from_utf8_lossy(&self.title);
        let title = title.trim_start();
        self.busy = title.starts_with(BUSY_MARKER);
        self.waiting = title.starts_with(WAIT_MARKER);
        self.title.clear();
    }

    /// True while the most recent title says a turn is running.
    pub fn working(&self) -> bool {
        self.busy
    }

    /// True while the most recent title says Hermes is waiting for the user's answer.
    pub fn waiting(&self) -> bool {
        self.waiting
    }

    /// A new session starts idle.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(chunks: &[&[u8]]) -> TitleActivity {
        let mut t = TitleActivity::default();
        for c in chunks {
            t.output(c);
        }
        t
    }

    #[test]
    fn busy_and_idle_titles_with_both_terminators() {
        assert!(feed(&["\x1b]0;⏳ chat · opus\x07".as_bytes()]).working());
        assert!(feed(&["\x1b]2;⏳ chat\x1b\\".as_bytes()]).working());
        assert!(feed(&["\x1b]1;⏳ chat\x07".as_bytes()]).working());
        assert!(!feed(&["\x1b]0;⏳ a\x07\x1b]0;✓ a\x07".as_bytes()]).working());
        assert!(!feed(&["\x1b]0;⚠ waiting\x07".as_bytes()]).working());
    }

    #[test]
    fn waiting_title_is_its_own_state_and_clears_on_the_next_title() {
        let mut t = feed(&["\x1b]0;⏳ chat\x07\x1b]2;⚠ chat · opus\x1b\\".as_bytes()]);
        assert!(t.waiting() && !t.working());
        t.output("\x1b]0;⏳ chat\x07".as_bytes());
        assert!(!t.waiting() && t.working());
        t.output("\x1b]0;⚠ again\x07\x1b]0;✓ chat\x07".as_bytes());
        assert!(!t.waiting() && !t.working());
        // A warning sign in the transcript is not a title.
        t.output("⚠ approval needed".as_bytes());
        assert!(!t.waiting());
        t.output("\x1b]0;⚠ x\x07".as_bytes());
        t.reset();
        assert!(!t.waiting());
    }

    #[test]
    fn title_split_across_reads_including_inside_the_marker() {
        let all = "redraw\x1b]0;⏳ chat\x07more".as_bytes();
        for cut in 0..all.len() {
            assert!(feed(&[&all[..cut], &all[cut..]]).working(), "cut at {cut}");
        }
    }

    #[test]
    fn idle_redraws_content_and_other_sequences_never_count() {
        let mut t = feed(&["\x1b]0;✓ chat\x07".as_bytes()]);
        for _ in 0..1000 {
            t.output("\x1b[2;1H\x1b[K status ⏳ 12s \x1b[0m".as_bytes());
        }
        t.output("\x1b]8;;https://x\x07link\x1b]8;;\x07\x1b]7;file:///home\x1b\\".as_bytes());
        assert!(!t.working());
    }

    #[test]
    fn over_long_titles_are_skipped_and_parser_recovers() {
        let mut long = "\x1b]0;⏳".as_bytes().to_vec();
        long.extend(std::iter::repeat_n(b'x', MAX_TITLE + 10));
        long.push(0x07);
        let mut t = feed(&[&long]);
        assert!(!t.working());
        t.output("\x1b]0;⏳ ok\x07".as_bytes());
        assert!(t.working());
        t.reset();
        assert!(!t.working());
    }
}
