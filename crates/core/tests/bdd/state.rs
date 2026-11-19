use std::fs;

use cucumber::{given, then, when};
use orchestrator_core::state;

use crate::{CoreWorld, read};

fn save(world: &CoreWorld) {
    state::save(&world.state_path(), &world.state).expect("save state");
}

#[given(expr = "repositories {string} and {string} are open as workspaces")]
fn open_as_workspaces(world: &mut CoreWorld, first: String, second: String) {
    for name in [first, second] {
        let path = world.new_repo(&name);
        state::add_workspace(&mut world.state, &path, None).expect("add workspace");
        save(world);
    }
}

#[given(expr = "{string} has a saved conversation id")]
fn saved_conversation_id(world: &mut CoreWorld, name: String) {
    let id = "6f0c7a52-3c2e-4d55-9a38-2f7a3d1f0b11".to_owned();
    let workspace = world.state.workspaces.iter_mut().find(|w| w.name == name).expect("workspace");
    workspace.claude_session_id = Some(id.clone());
    world.saved_id = Some(id);
    save(world);
}

#[given(expr = "{string} has the notes {string}")]
fn saved_notes(world: &mut CoreWorld, name: String, notes: String) {
    let id = world.state.workspaces.iter().find(|w| w.name == name).expect("workspace").id.clone();
    state::set_notes(&mut world.state, &id, notes).expect("set notes");
    save(world);
}

#[given("a saved state file that is not valid JSON")]
fn invalid_state_file(world: &mut CoreWorld) {
    let path = world.state_path();
    let text = "{ \"workspaces\": [ not json".to_owned();
    fs::create_dir_all(path.parent().expect("parent")).expect("create state folder");
    fs::write(&path, &text).expect("write state");
    world.state_text = Some(text);
}

#[when(regex = r"^the saved state is loaded(?: again)?$")]
fn load_state(world: &mut CoreWorld) {
    if let Some(loaded) = world.record(state::load(&world.state_path())) {
        world.state = loaded;
    }
}

#[then(expr = "the workspaces {string} and {string} are listed")]
fn workspaces_listed(world: &mut CoreWorld, first: String, second: String) {
    let names: Vec<&str> = world.state.workspaces.iter().map(|w| w.name.as_str()).collect();
    assert_eq!(names, [first.as_str(), second.as_str()]);
}

#[then(expr = "{string} still has its conversation id")]
fn still_has_id(world: &mut CoreWorld, name: String) {
    let workspace = world.state.workspaces.iter().find(|w| w.name == name).expect("workspace");
    assert_eq!(workspace.claude_session_id, world.saved_id);
}

#[then(expr = "{string} still has the notes {string}")]
fn still_has_notes(world: &mut CoreWorld, name: String, notes: String) {
    let workspace = world.state.workspaces.iter().find(|w| w.name == name).expect("workspace");
    assert_eq!(workspace.notes, notes);
}

#[then("loading fails because the state file is not valid")]
fn load_failed(world: &mut CoreWorld) {
    world.assert_refused("Json");
}

#[then("the state file is unchanged")]
fn state_unchanged(world: &mut CoreWorld) {
    assert_eq!(Some(read(&world.state_path())), world.state_text);
}
