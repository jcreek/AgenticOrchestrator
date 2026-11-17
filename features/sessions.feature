Feature: Sessions and tabs
  Each repository folder or worktree runs at most one Claude Code session, shown as a tab
  grouped under its repository. After a restart each session can resume its conversation or start fresh.

  @ui
  Scenario: Starting a session in a repository folder
    Given a repository "alpha"
    When I start a new session in the repository folder of "alpha"
    Then a tab named "alpha" on the branch "main" is shown
    And Claude Code is running in "alpha"

  @core
  Scenario: A repository and its worktrees belong to one group
    Given a repository "alpha"
    And a worktree of "alpha" on the new branch "feature"
    And a repository "beta"
    Then the worktree "feature" is in the "alpha" group
    And "beta" is in a group of its own named "beta"

  @core
  Scenario: The same folder cannot be opened twice
    Given a repository "alpha" open as a workspace
    When I open "alpha" as a workspace again, spelled in upper case
    Then it is refused because the folder is already open
    And there is still one workspace

  @ui
  Scenario: A second session in the same folder is refused
    Given a session open in repository "alpha"
    When I ask for a new session in "alpha"
    Then running in the repository folder of "alpha" is not offered because it is already open in a tab

  @ui
  Scenario: Restored sessions offer to resume or start fresh
    Given a session open in repository "alpha"
    When the app is restarted
    Then the "alpha" session offers "Resume conversation" and "Start fresh"

  @ui
  Scenario Outline: Resuming keeps the conversation, starting fresh begins a new one
    Given a session open in repository "alpha"
    And the app is restarted
    When I choose "<choice>" in the "alpha" session
    Then Claude Code in "alpha" is started with <conversation>

    Examples:
      | choice              | conversation             |
      | Resume conversation | the same conversation id |
      | Start fresh         | a new conversation id    |
