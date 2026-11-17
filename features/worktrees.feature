Feature: Worktrees
  A session can run in its own worktree beside the repository, on its own branch. Removing a worktree
  never loses work silently: the app lists what would be lost, removes only if that is still all,
  always keeps the branch, and refuses worktrees it cannot remove safely.

  @core
  Scenario: A new worktree sits beside the repository on its own branch
    Given a repository "alpha" with a remote
    When I create a worktree of "alpha" on the new branch "feature/login"
    Then the worktree is in the folder "alpha.worktrees/feature-login" beside "alpha"
    And the worktree is on the branch "feature/login"
    And the branch "feature/login" does not track "main"

  @core
  Scenario Outline: Hostile or invalid branch names are refused
    Given a repository "alpha" with a remote
    When I create a worktree of "alpha" on the new branch "<name>"
    Then it is refused as an invalid branch name
    And "alpha" still has only its main worktree

    Examples:
      | name       |
      | -x         |
      | --force    |
      | ../../evil |
      | has space  |
      | @{-1}      |
      |            |

  @core
  Scenario: Files listed in .worktreeinclude are copied into a new worktree
    Given a repository "alpha" with a remote
    And "alpha" has the untracked files ".env.local", "config/dev.json" and "notes.md"
    And the ".worktreeinclude" of "alpha" lists ".env.local" and "config/*.json"
    When I create a worktree of "alpha" on the new branch "with-env"
    Then the worktree has ".env.local" and "config/dev.json"
    And the worktree does not have "notes.md"

  @core
  Scenario: Removal lists everything that would be lost
    Given a repository "alpha" with a remote
    And a worktree of "alpha" on the new branch "dirty"
    When in the worktree a tracked file is modified, a file is added, a log file is written and a commit is made on a detached HEAD
    Then removing the worktree would lose:
      | what             | count |
      | modified files   | 1     |
      | untracked files  | 1     |
      | ignored files    | 1     |
      | unpushed commits | 1     |

  @core
  Scenario: Removal proceeds only if what would be lost is unchanged since it was shown
    Given a repository "alpha" with a remote
    And a worktree of "alpha" on the new branch "race"
    And a log file is written in the worktree
    And I have been shown what removing the worktree would lose
    When Claude creates "agent-work.txt" in the worktree
    And I confirm the removal
    Then the removal is refused because the worktree changed
    And the worktree still has "agent-work.txt"

  @core
  Scenario: The branch always survives removal
    Given a repository "alpha" with a remote
    And a worktree of "alpha" on the new branch "done"
    And a commit is made in the worktree
    And I have been shown what removing the worktree would lose
    When I confirm the removal
    Then the worktree folder is gone
    And the branch "done" still has that commit

  @core
  Scenario: The main worktree and unrelated folders are never removed
    Given a repository "alpha" with a remote
    When I try to remove the main folder of "alpha" as a worktree
    Then the removal is refused because it is the main worktree
    When I try to remove an unrelated folder as a worktree of "alpha"
    Then the removal is refused because it is not a worktree of "alpha"

  @core
  Scenario: A worktree that contains another worktree is refused
    Given a repository "alpha" with a remote
    And a worktree of "alpha" on the new branch "outer"
    And another worktree "inner" with uncommitted work inside it
    And I have been shown what removing the worktree would lose
    When I confirm the removal
    Then the removal is refused because the worktree contains another worktree
    And the uncommitted work in "inner" is still there

  @core
  Scenario: A worktree with initialised submodules is refused
    Given a repository "alpha" with a remote and a submodule "sub"
    And a worktree of "alpha" on the new branch "with-sub" with its submodules initialised
    And I have been shown what removing the worktree would lose
    When I confirm the removal
    Then the removal is refused because the worktree has submodules
    And the worktree still has "sub/s.txt"

  @core
  Scenario: A worktree whose emptied submodule still holds unpushed commits is refused
    Given a repository "alpha" with a remote and a submodule "sub"
    And a worktree of "alpha" on the new branch "emptied" with its submodules initialised
    And the submodule "sub" in the worktree has a commit that is not pushed
    And the submodule folder "sub" in the worktree is emptied
    And I have been shown what removing the worktree would lose
    When I confirm the removal
    Then the removal is refused because the worktree has submodules
    And the worktree's git data for submodule "sub" is still there

  @core
  Scenario Outline: A junction inside a removed worktree never empties its target
    Given a repository "alpha" with a remote
    And a worktree of "alpha" on the new branch "linked"
    And a folder outside the worktree holding "keep.txt"
    And a junction at "<junction>" in the worktree pointing to that folder
    And I have been shown what removing the worktree would lose
    When I confirm the removal
    Then the worktree folder is gone
    And the folder outside still holds "keep.txt"

    Examples:
      | junction                  |
      | shared                    |
      | node_modules/@scope/pkg   |

  @ui
  Scenario: Closing a session and discarding its worktree
    Given a session open in a new worktree of "alpha" on the branch "spike"
    And "scratch.txt" is created in the "spike" worktree
    When I ask to close the "spike" session
    Then I am told that removing the worktree loses "1 untracked file"
    When I choose to discard the changes and remove the worktree
    Then the "spike" worktree folder is gone
    And the branch "spike" still exists in "alpha"

  @ui
  Scenario: Removal is not offered for a folder the app did not create
    Given a session open in repository "alpha"
    When I ask to close the "alpha" session
    Then I am not offered to remove anything
