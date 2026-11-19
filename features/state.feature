Feature: Saved state
  The workspace list and each session's conversation id and notes survive a restart. A state file
  the app cannot read is reported and never overwritten, so the user can repair it.

  @core
  Scenario: Workspaces are remembered across a restart
    Given repositories "alpha" and "beta" are open as workspaces
    And "alpha" has a saved conversation id
    And "alpha" has the notes "Halfway through the login fix"
    When the saved state is loaded again
    Then the workspaces "alpha" and "beta" are listed
    And "alpha" still has its conversation id
    And "alpha" still has the notes "Halfway through the login fix"

  @core
  Scenario: An unreadable state file is reported and left untouched
    Given a saved state file that is not valid JSON
    When the saved state is loaded
    Then loading fails because the state file is not valid
    And the state file is unchanged

  @ui
  Scenario: The app reports an unreadable state file and never overwrites it
    Given a saved state file that is not valid JSON
    When the app starts
    Then I am told "Saved sessions could not be read"
    When I try to add a workspace for repository "alpha"
    Then it is refused because the saved state cannot be read
    And the state file is unchanged
