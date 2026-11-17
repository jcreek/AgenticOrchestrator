Feature: Leaving safely
  Closing a session or the window never stops Claude Code or loses unsaved edits without asking first,
  and asks nothing when there is nothing at stake.

  @ui
  Scenario: Closing a session with unsaved edits warns
    Given a session open in repository "alpha"
    And I am editing "notes.txt" in "alpha"
    And I type "unsaved words" into the editor
    When I ask to close the "alpha" session
    Then I am warned "Unsaved edits in the editor will be lost."

  @ui
  Scenario: Closing the window with a running session asks first
    Given a session open in repository "alpha"
    When I close the window
    Then I am asked "Close Agentic Orchestrator?"
    And I am warned "1 running Claude session will be stopped"

  @ui
  Scenario: Closing the window with unsaved edits asks first
    Given a session open in repository "alpha"
    And I am editing "notes.txt" in "alpha"
    And I type "unsaved words" into the editor
    When I close the window
    Then I am warned "1 file has unsaved edits that will be lost."

  @ui
  Scenario: Cancelling keeps everything alive
    Given a session open in repository "alpha"
    When I close the window
    And I cancel
    Then no dialog is open
    And Claude Code is running in "alpha"
    And what I type into Claude's terminal in "alpha" still reaches Claude

  @ui
  Scenario: Confirming stops the sessions and closes the app
    Given a session open in repository "alpha"
    When I close the window
    And I choose "Close app"
    Then the app has closed
    And Claude Code and every process started for "alpha" have stopped

  @ui
  Scenario: With nothing at stake the window closes immediately
    Given the app is open with no sessions
    When I close the window
    Then the app has closed
