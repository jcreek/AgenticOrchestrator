Feature: Many sessions
  The title bar stays usable however many sessions are open: the window controls, Overview and
  New session never get pushed out, sessions that do not fit are one click away, and the session
  being worked on is always in view.

  @ui
  Scenario: A repository with one session shows one tab named after it; more sessions get a group label
    Given a session open in repository "alpha"
    And a session open in repository "beta"
    And a session open in a new worktree of "beta" on the branch "feature"
    Then "alpha" is a single tab named "alpha" without a group label
    And the "beta" group has a label followed by tabs for "main" and "feature"

  @ui
  Scenario: Collapsing a repository group hides its tabs and shows how many it holds
    Given a session open in repository "alpha"
    And a session open in repository "beta"
    And a session open in a new worktree of "beta" on the branch "feature"
    And I am viewing the "alpha" session
    When I collapse the "beta" group
    Then the "beta" group shows only its label with 2 sessions
    When I expand the "beta" group
    Then the "beta" group has a label followed by tabs for "main" and "feature"

  @ui
  Scenario Outline: The window controls stay fully visible and usable with many sessions open
    Given 32 folders are open as sessions
    And the window is <width> pixels wide
    Then the Overview, New session, minimise, maximise and close buttons are fully visible and not covered
    When I press the New session button
    Then I am asked "New session"

    Examples:
      | width |
      | 1400  |
      | 900   |

  @ui
  Scenario: Sessions that do not fit are listed in the overflow menu
    Given 32 folders are open as sessions
    And the window is 1400 pixels wide
    And Claude Code asks a question in "folder-30", "folder-31" and "folder-32"
    And I am viewing the "folder-01" session
    Then the overflow button says how many sessions are hidden and that 3 of them need me
    When I open the overflow menu
    Then it lists all 32 sessions with their status
    When I choose "folder-31" from the menu
    Then "folder-31" is the active session
    And the tab of the active session is fully visible

  @ui
  Scenario: Dragging a tab onto another moves it there, and the order is kept after a restart
    Given 3 folders are open as sessions
    When I drag the "folder-01" tab onto the "folder-03" tab
    Then the tabs are in the order "folder-02", "folder-03", "folder-01"
    And "folder-01" is the active session
    When I close and reopen the app
    Then the tabs are in the order "folder-02", "folder-03", "folder-01"

  @ui
  Scenario Outline: The active tab is fully visible after switching sessions by keyboard
    Given 32 folders are open as sessions
    And the window is 900 pixels wide
    When I switch to the <which> session with the keyboard
    Then the tab of the active session is fully visible

    Examples:
      | which  |
      | first  |
      | last   |
      | 16th   |
