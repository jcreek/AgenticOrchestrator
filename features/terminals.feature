Feature: Terminals
  Terminals live in the app, not the page: their output survives tab switches and window reloads,
  closing a session ends everything it started, and keys typed in a terminal belong to the program in it.

  @core
  Scenario: A new viewer sees everything the terminal printed so far
    Given a terminal that printed 200 numbered lines and exited
    When a second viewer attaches to it
    Then the second viewer sees exactly what the first viewer saw

  @core
  Scenario: Ending a terminal ends its whole process tree
    Given a terminal running a command that started a long-running child process
    When the terminal is closed
    Then the terminal and its child process have both stopped

  @core
  Scenario: A shell that exits straight away is reported as exited with its code
    When a terminal starts a shell that exits at once with code 3
    Then the terminal is reported as exited with code 3

  @ui
  Scenario: Output is kept when switching tabs
    Given sessions open in repositories "alpha" and "beta"
    And I type "hello-alpha" into Claude's terminal in "alpha"
    When I view the "beta" session
    And I view the "alpha" session
    Then Claude's terminal in "alpha" shows "hello-alpha"

  @ui
  Scenario: Output is kept when the window reloads
    Given a session open in repository "alpha"
    And I type "hello-alpha" into Claude's terminal in "alpha"
    When the window reloads
    And I view the "alpha" session
    Then Claude's terminal in "alpha" shows "hello-alpha"
    And Claude's terminal in "alpha" shows "new session"

  @ui
  Scenario: Closing a session ends Claude Code and everything started in its shells
    Given a session open in repository "alpha"
    And a shell in "alpha" is running a long command
    When I close the "alpha" session and confirm
    Then Claude Code and every process started for "alpha" have stopped

  @ui
  Scenario: A shell that exits straight away shows as exited
    Given the shell is a program that exits straight away
    And a session open in repository "alpha"
    When I open a new shell in "alpha"
    Then the shell tab shows that it exited

  @ui
  Scenario: Ctrl+W in a terminal goes to the program, not to the app
    Given a session open in repository "alpha"
    When I press Ctrl+W in Claude's terminal in "alpha"
    Then Claude's terminal in "alpha" shows "<Ctrl+W>"
    And no dialog is open

  @ui
  Scenario: Ctrl+W outside a terminal asks to close the session
    Given a session open in repository "alpha"
    When I press Ctrl+W outside the terminal
    Then I am asked "Close main?"
