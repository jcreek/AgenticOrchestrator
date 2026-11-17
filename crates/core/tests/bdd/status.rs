use cucumber::{given, then, when};
use orchestrator_core::status::{AgentEntry, SessionStatus, find_entry, terminal_status};

use crate::CoreWorld;

fn named(status: &str) -> SessionStatus {
    match status {
        "starting" | "Starting" => SessionStatus::Starting,
        "working" | "Working" => SessionStatus::Working,
        "Needs permission" => SessionStatus::NeedsPermission,
        "needing input" | "Waiting for input" => SessionStatus::NeedsInput,
        "finished" | "Finished" => SessionStatus::DoneUnseen,
        "idle" | "Idle" => SessionStatus::Idle,
        "Exited" => SessionStatus::Exited,
        other => panic!("unknown status {other:?}"),
    }
}

fn reported(activity: &str) -> Option<AgentEntry> {
    let (status, waiting_for) = match activity {
        "is busy" => ("busy", None),
        "is waiting for a permission prompt" => ("waiting", Some("permission prompt")),
        "is waiting for a question" => ("waiting", Some("question")),
        "is idle" => ("idle", None),
        "is not listed" => return None,
        other => panic!("unknown report {other:?}"),
    };
    Some(AgentEntry {
        session_id: "s-1".into(),
        cwd: r"C:\repos\alpha".into(),
        status: Some(status.into()),
        waiting_for: waiting_for.map(Into::into),
    })
}

fn previous(world: &CoreWorld) -> SessionStatus {
    world.status.expect("the session's status was set earlier")
}

#[given(regex = r"^a session that is (starting|working|idle|needing input|finished)$")]
fn a_session_that_is(world: &mut CoreWorld, status: String) {
    world.status = Some(named(&status));
}

#[when(regex = r"^Claude Code reports it (.+) while its tab is (in view|not in view)$")]
fn claude_reports(world: &mut CoreWorld, activity: String, view: String) {
    let entry = reported(&activity);
    world.status = Some(terminal_status(previous(world), entry.as_ref(), true, view == "in view"));
}

#[when("Claude Code lists the only session in its folder under a new id, as busy")]
fn listed_under_a_new_id(world: &mut CoreWorld) {
    let agents: Vec<AgentEntry> = reported("is busy").into_iter().collect();
    let entry = find_entry(&agents, Some("s-saved"), r"C:\repos\alpha", &["s-saved"]);
    world.status = Some(terminal_status(previous(world), entry, true, false));
}

#[when("its Claude Code process ends while still reported as busy")]
fn process_ends(world: &mut CoreWorld) {
    let entry = reported("is busy");
    world.status = Some(terminal_status(previous(world), entry.as_ref(), false, false));
}

#[then(regex = r"^the session is (.+)$")]
fn session_is(world: &mut CoreWorld, status: String) {
    let found = previous(world);
    assert_eq!(found, named(&status), "the session is {found:?}, expected {status}");
}
