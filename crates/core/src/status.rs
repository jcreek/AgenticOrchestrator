use std::time::Duration;

use serde::{Deserialize, Serialize};

const PERMISSION_KEYWORD: &str = "permission";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentEntry {
    pub session_id: String,
    pub cwd: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub waiting_for: Option<String>,
}

/// Parses the output of `claude agents --json`, skipping entries that lack a session id
/// so that one unexpected entry cannot blank every tab's status.
pub fn parse_agents(json: &str) -> Result<Vec<AgentEntry>, serde_json::Error> {
    let entries: Vec<serde_json::Value> = serde_json::from_str(json)?;
    Ok(entries
        .into_iter()
        .filter_map(|entry| serde_json::from_value(entry).ok())
        .collect())
}

/// The entry with the tab's session id, or else the only entry in its folder that no tab claims,
/// because Claude Code can relist a running session under a new id.
pub fn find_entry<'a>(
    agents: &'a [AgentEntry],
    session_id: Option<&str>,
    folder: &str,
    claimed_session_ids: &[&str],
) -> Option<&'a AgentEntry> {
    if let Some(entry) = agents.iter().find(|agent| Some(agent.session_id.as_str()) == session_id) {
        return Some(entry);
    }
    let mut unclaimed_in_folder = agents
        .iter()
        .filter(|agent| !claimed_session_ids.contains(&agent.session_id.as_str()) && same_folder(&agent.cwd, folder));
    let only = unclaimed_in_folder.next()?;
    unclaimed_in_folder.next().is_none().then_some(only)
}

fn same_folder(left: &str, right: &str) -> bool {
    let trimmed = |path: &str| path.trim_end_matches(['\\', '/']).to_lowercase();
    trimmed(left) == trimmed(right)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Observed {
    Busy,
    WaitingForPermission,
    WaitingForInput,
    Idle,
    NotListed,
}

pub fn observe(entry: Option<&AgentEntry>) -> Observed {
    let Some(entry) = entry else {
        return Observed::NotListed;
    };
    match entry.status.as_deref() {
        Some("busy") => Observed::Busy,
        Some("waiting") => {
            let waiting_for = entry.waiting_for.as_deref().unwrap_or_default();
            if waiting_for.to_ascii_lowercase().contains(PERMISSION_KEYWORD) {
                Observed::WaitingForPermission
            } else {
                Observed::WaitingForInput
            }
        }
        _ => Observed::Idle,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum SessionStatus {
    Starting,
    Working,
    NeedsPermission,
    NeedsInput,
    DoneUnseen,
    Idle,
    Exited,
}

impl SessionStatus {
    fn was_active(self) -> bool {
        matches!(
            self,
            Self::Working | Self::NeedsPermission | Self::NeedsInput | Self::DoneUnseen
        )
    }

    /// Lower ranks are more urgent; `None` means the tab does not need the user.
    pub fn attention_rank(self) -> Option<u8> {
        match self {
            Self::NeedsPermission => Some(0),
            Self::NeedsInput => Some(1),
            Self::DoneUnseen => Some(2),
            Self::Starting | Self::Working | Self::Idle | Self::Exited => None,
        }
    }
}

pub fn next_status(previous: SessionStatus, observed: Observed, tab_is_viewed: bool) -> SessionStatus {
    match observed {
        Observed::Busy => SessionStatus::Working,
        Observed::WaitingForPermission => SessionStatus::NeedsPermission,
        Observed::WaitingForInput => SessionStatus::NeedsInput,
        Observed::Idle if !tab_is_viewed && previous.was_active() => SessionStatus::DoneUnseen,
        Observed::Idle => SessionStatus::Idle,
        Observed::NotListed if previous == SessionStatus::Starting => SessionStatus::Starting,
        Observed::NotListed => SessionStatus::Exited,
    }
}

/// The status once the user looks at the tab: finished work stops being unseen.
pub fn when_viewed(status: SessionStatus) -> SessionStatus {
    if status == SessionStatus::DoneUnseen { SessionStatus::Idle } else { status }
}

/// Status of a Claude terminal: an exited process is `Exited` whatever the CLI last reported, and
/// a running one the CLI does not list (its session id changed, or a gap) keeps its status.
pub fn terminal_status(
    previous: SessionStatus,
    entry: Option<&AgentEntry>,
    running: bool,
    tab_is_viewed: bool,
) -> SessionStatus {
    match entry {
        _ if !running => SessionStatus::Exited,
        None if previous == SessionStatus::Exited => SessionStatus::Starting,
        None => previous,
        Some(_) => next_status(previous, observe(entry), tab_is_viewed),
    }
}

/// How long after a wake the poller keeps the fast cadence.
pub const SETTLE_AFTER_WAKE: Duration = Duration::from_secs(6);

/// True while a session is starting or working, so a permission prompt must be noticed promptly, and
/// shortly after a wake, because the CLI can report a change a moment after the output that caused it.
pub fn needs_fast_poll(statuses: impl IntoIterator<Item = SessionStatus>, since_wake: Duration) -> bool {
    since_wake < SETTLE_AFTER_WAKE
        || statuses
            .into_iter()
            .any(|status| matches!(status, SessionStatus::Starting | SessionStatus::Working))
}

/// True when a change should alert a user who is not looking: newly needing permission or input.
pub fn newly_needs_user(previous: SessionStatus, next: SessionStatus) -> bool {
    previous != next && matches!(next, SessionStatus::NeedsPermission | SessionStatus::NeedsInput)
}

/// Picks the most urgent tab other than the current one, preferring the next tab
/// to the right (wrapping) among equally urgent ones.
pub fn next_needing_attention(statuses: &[SessionStatus], current: usize) -> Option<usize> {
    let count = statuses.len();
    (1..count)
        .map(|offset| (current + offset) % count)
        .filter_map(|index| statuses[index].attention_rank().map(|rank| (rank, index)))
        .min_by_key(|(rank, _)| *rank)
        .map(|(_, index)| index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewing_a_tab_clears_finished_and_nothing_else() {
        assert_eq!(when_viewed(SessionStatus::DoneUnseen), SessionStatus::Idle);
        for status in [SessionStatus::Working, SessionStatus::NeedsPermission, SessionStatus::NeedsInput, SessionStatus::Exited] {
            assert_eq!(when_viewed(status), status);
        }
    }

    const AGENTS_JSON: &str = r#"[
        {"id":"bg1","cwd":"C:\\repos\\alpha","kind":"background","startedAt":"2026-11-16T09:00:00Z","sessionId":"s-bg","name":"bg","state":"blocked"},
        {"pid":101,"cwd":"C:\\repos\\alpha","kind":"interactive","startedAt":"2026-11-16T09:01:00Z","sessionId":"s-idle","name":"a","status":"idle"},
        {"pid":102,"cwd":"C:\\repos\\beta","kind":"interactive","startedAt":"2026-11-16T09:02:00Z","sessionId":"s-perm","name":"b","status":"waiting","waitingFor":"permission prompt"},
        {"pid":103,"cwd":"C:\\repos\\gamma","kind":"interactive","startedAt":"2026-11-16T09:03:00Z","sessionId":"s-busy","name":"c","status":"busy"},
        {"pid":104,"cwd":"C:\\repos\\delta","kind":"interactive","startedAt":"2026-11-16T09:04:00Z","sessionId":"s-ask","name":"d","status":"waiting","waitingFor":"question"},
        {"pid":105,"kind":"interactive"}
    ]"#;

    fn observed_for(session_id: &str) -> Observed {
        let agents = parse_agents(AGENTS_JSON).expect("fixture parses");
        observe(agents.iter().find(|agent| agent.session_id == session_id))
    }

    #[test]
    fn parse_skips_entries_without_a_session_id() {
        assert_eq!(parse_agents(AGENTS_JSON).expect("fixture parses").len(), 5);
    }

    #[test]
    fn parse_rejects_output_that_is_not_a_json_array() {
        assert!(parse_agents("claude: command failed").is_err());
    }

    #[test]
    fn observe_maps_each_reported_status() {
        assert_eq!(observed_for("s-busy"), Observed::Busy);
        assert_eq!(observed_for("s-idle"), Observed::Idle);
        assert_eq!(observed_for("s-perm"), Observed::WaitingForPermission);
        assert_eq!(observed_for("s-ask"), Observed::WaitingForInput);
        assert_eq!(observed_for("s-unknown"), Observed::NotListed);
    }

    #[test]
    fn entry_without_a_status_field_is_observed_as_idle() {
        assert_eq!(observed_for("s-bg"), Observed::Idle);
    }

    fn found(session_id: &str, folder: &str, claimed: &[&str]) -> Option<String> {
        let agents = parse_agents(AGENTS_JSON).expect("fixture parses");
        find_entry(&agents, Some(session_id), folder, claimed).map(|entry| entry.session_id.clone())
    }

    #[test]
    fn entry_with_the_session_id_wins_over_others_in_the_folder() {
        assert_eq!(found("s-idle", r"C:\repos\alpha", &[]).as_deref(), Some("s-idle"));
    }

    #[test]
    fn relisted_session_is_found_by_its_folder() {
        assert_eq!(found("s-gone", r"c:\REPOS\gamma\", &[]).as_deref(), Some("s-busy"));
    }

    #[test]
    fn folder_match_is_refused_when_ambiguous_or_claimed_by_another_tab() {
        assert_eq!(found("s-gone", r"C:\repos\alpha", &[]), None);
        assert_eq!(found("s-gone", r"C:\repos\gamma", &["s-busy"]), None);
        assert_eq!(found("s-gone", r"C:\repos\nowhere", &[]), None);
    }

    #[test]
    fn finishing_work_on_a_background_tab_is_done_unseen() {
        let status = next_status(SessionStatus::Working, Observed::Idle, false);
        assert_eq!(status, SessionStatus::DoneUnseen);
    }

    #[test]
    fn finishing_work_on_the_viewed_tab_is_idle() {
        let status = next_status(SessionStatus::Working, Observed::Idle, true);
        assert_eq!(status, SessionStatus::Idle);
    }

    #[test]
    fn done_unseen_stays_until_the_tab_is_viewed() {
        let unseen = next_status(SessionStatus::DoneUnseen, Observed::Idle, false);
        assert_eq!(unseen, SessionStatus::DoneUnseen);
        assert_eq!(next_status(unseen, Observed::Idle, true), SessionStatus::Idle);
    }

    #[test]
    fn idle_session_that_stays_idle_is_not_marked_done() {
        let status = next_status(SessionStatus::Idle, Observed::Idle, false);
        assert_eq!(status, SessionStatus::Idle);
    }

    #[test]
    fn session_not_yet_listed_keeps_starting() {
        let status = next_status(SessionStatus::Starting, Observed::NotListed, false);
        assert_eq!(status, SessionStatus::Starting);
    }

    #[test]
    fn session_that_disappears_after_running_is_exited() {
        let status = next_status(SessionStatus::Working, Observed::NotListed, false);
        assert_eq!(status, SessionStatus::Exited);
    }

    #[test]
    fn answering_a_permission_prompt_returns_to_working() {
        let status = next_status(SessionStatus::NeedsPermission, Observed::Busy, true);
        assert_eq!(status, SessionStatus::Working);
    }

    #[test]
    fn exited_process_wins_over_a_listed_session() {
        let agents = parse_agents(AGENTS_JSON).expect("fixture parses");
        let busy = agents.iter().find(|agent| agent.session_id == "s-busy");
        assert_eq!(terminal_status(SessionStatus::Working, busy, false, false), SessionStatus::Exited);
        assert_eq!(terminal_status(SessionStatus::Starting, busy, true, false), SessionStatus::Working);
        assert_eq!(terminal_status(SessionStatus::Starting, None, true, false), SessionStatus::Starting);
    }

    #[test]
    fn running_session_missing_from_the_list_keeps_its_status_and_does_not_re_alert() {
        use SessionStatus::{Exited, NeedsInput, Starting, Working};
        let agents = parse_agents(AGENTS_JSON).expect("fixture parses");
        let asking = agents.iter().find(|agent| agent.session_id == "s-ask");
        for previous in [Starting, Working, NeedsInput] {
            assert_eq!(terminal_status(previous, None, true, false), previous);
        }
        assert_eq!(terminal_status(Exited, None, true, false), Starting);
        let gap = terminal_status(NeedsInput, None, true, false);
        assert!(!newly_needs_user(gap, terminal_status(gap, asking, true, false)));
    }

    #[test]
    fn only_entering_permission_or_input_needs_the_user() {
        use SessionStatus::{DoneUnseen, NeedsInput, NeedsPermission, Working};
        assert!(newly_needs_user(Working, NeedsPermission));
        assert!(newly_needs_user(NeedsPermission, NeedsInput));
        assert!(!newly_needs_user(NeedsInput, NeedsInput));
        assert!(!newly_needs_user(Working, DoneUnseen));
    }

    #[test]
    fn fast_polling_while_starting_or_working_or_settling_after_a_wake() {
        use SessionStatus::{DoneUnseen, Exited, Idle, NeedsInput, NeedsPermission, Starting, Working};
        let settled = SETTLE_AFTER_WAKE;
        assert!(!needs_fast_poll([], settled));
        assert!(!needs_fast_poll([Idle, DoneUnseen, NeedsInput, NeedsPermission, Exited], settled));
        assert!(needs_fast_poll([Idle, Starting], settled));
        assert!(needs_fast_poll([Working, Exited], settled));
        assert!(needs_fast_poll([Idle, NeedsPermission], Duration::ZERO));
        assert!(needs_fast_poll([], SETTLE_AFTER_WAKE - Duration::from_millis(1)));
    }

    #[test]
    fn attention_prefers_permission_over_input_over_done() {
        use SessionStatus::{DoneUnseen, Idle, NeedsInput, NeedsPermission, Working};
        let statuses = [Idle, DoneUnseen, NeedsInput, Working, NeedsPermission];
        assert_eq!(next_needing_attention(&statuses, 0), Some(4));
    }

    #[test]
    fn attention_takes_the_next_tab_to_the_right_among_equals() {
        use SessionStatus::{Idle, NeedsInput};
        let statuses = [NeedsInput, Idle, NeedsInput, NeedsInput];
        assert_eq!(next_needing_attention(&statuses, 2), Some(3));
        assert_eq!(next_needing_attention(&statuses, 3), Some(0));
    }

    #[test]
    fn attention_ignores_the_current_tab_and_quiet_tabs() {
        use SessionStatus::{Exited, Idle, NeedsPermission, Starting, Working};
        assert_eq!(next_needing_attention(&[NeedsPermission, Idle, Working], 0), None);
        assert_eq!(next_needing_attention(&[Starting, Exited], 0), None);
        assert_eq!(next_needing_attention(&[], 0), None);
    }
}
