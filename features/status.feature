Feature: Session status and attention
  Each tab shows what its Claude Code session is doing, so the user can see at a glance which
  sessions need them: a permission prompt first, then a question, then finished work they have not seen.

  @core
  Scenario Outline: What Claude Code reports becomes the session's status
    Given a session that is <before>
    When Claude Code reports it <reported> while its tab is <view>
    Then the session is <status>

    Examples:
      | before          | reported                         | view        | status            |
      | starting        | is busy                          | in view     | Working           |
      | working         | is waiting for a permission prompt | in view   | Needs permission  |
      | working         | is waiting for a question        | in view     | Waiting for input |
      | working         | is idle                          | not in view | Finished          |
      | working         | is idle                          | in view     | Idle              |
      | idle            | is idle                          | not in view | Idle              |
      | starting        | is not listed                    | not in view | Starting          |
      | needing input   | is not listed                    | not in view | Waiting for input |

  @core
  Scenario: A session Claude Code lists under a new id is still followed
    Given a session that is needing input
    When Claude Code lists the only session in its folder under a new id, as busy
    Then the session is Working

  @core
  Scenario: Finished clears once the tab is viewed
    Given a session that is finished
    When Claude Code reports it is idle while its tab is in view
    Then the session is Idle

  @core
  Scenario: A session whose process has ended is exited, whatever was last reported
    Given a session that is working
    When its Claude Code process ends while still reported as busy
    Then the session is Exited

  @ui
  Scenario Outline: The tab shows what Claude Code is doing
    Given a session open in repository "alpha"
    And a session open in repository "beta"
    When Claude in "alpha" <activity>
    Then the "alpha" tab shows "<label>"

    Examples:
      | activity                        | label             |
      | starts working                  | Working           |
      | asks for permission             | Needs permission  |
      | asks a question                 | Waiting for input |
      | starts working and then finishes | Finished         |
      | exits with code 0               | Exited            |
      | exits with code 1               | Crashed · exit 1  |

  @ui
  Scenario: Finished is not shown for the tab I am looking at
    Given a session open in repository "alpha"
    When Claude in "alpha" starts working and then finishes
    Then the "alpha" tab shows "Idle"

  @ui
  Scenario: Finished clears when I view the tab
    Given a session open in repository "alpha"
    And a session open in repository "beta"
    And Claude in "alpha" starts working and then finishes
    And the "alpha" tab shows "Finished"
    When I view the "alpha" session
    Then the "alpha" tab shows "Idle"
    When I view the "beta" session
    Then the "alpha" tab shows "Idle"

  @ui
  Scenario: A session stuck before Claude Code reports it asks me to check the terminal
    Given Claude Code does not report new sessions
    When I start a new session in the repository folder of "alpha"
    Then the "alpha" tab shows "Starting"
    And after a while the "alpha" tab shows "Check terminal"

  @ui
  Scenario: Jumping to the sessions that need me, most urgent first
    Given sessions open in repositories "alpha", "beta", "gamma" and "delta"
    And I am viewing the "alpha" session
    And Claude in "beta" starts working and then finishes
    And Claude in "gamma" asks a question
    And Claude in "delta" asks for permission
    And the "beta" tab shows "Finished"
    And the "gamma" tab shows "Waiting for input"
    And the "delta" tab shows "Needs permission"
    When I jump to the next session needing me 4 times
    Then I visit "delta", "gamma", "beta" and "delta" in that order

  @ui
  Scenario: The overview badge counts the sessions that need me
    Given sessions open in repositories "alpha", "beta" and "gamma"
    When Claude in "alpha" asks for permission
    And Claude in "beta" asks a question
    And Claude in "gamma" starts working
    Then the "gamma" tab shows "Working"
    And the overview shows that 2 sessions need me

  @ui
  Scenario Outline: Status is unknown while Claude Code cannot report it, and the terminal keeps working
    Given a session open in repository "alpha"
    And Claude in "alpha" starts working
    When Claude Code's status report <failure>
    Then the "alpha" tab shows "Status unknown"
    And what I type into Claude's terminal in "alpha" still reaches Claude
    When Claude Code's status report works again
    Then the "alpha" tab shows "Working"

    Examples:
      | failure            |
      | fails              |
      | stops responding   |
