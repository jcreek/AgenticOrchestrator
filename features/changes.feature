Feature: Reading and editing changes
  The IDE pane shows what changed and lets the user edit, stage, commit and push. A save never
  overwrites work it has not seen, and never writes outside the workspace or into git's own files.

  @core
  Scenario: The changes list shows each kind of change with line counts
    Given a repository "alpha" with "edit.txt" of 3 lines and "gone.txt" of 2 lines committed
    When 2 lines are added to "edit.txt" and 1 is removed
    And "gone.txt" is deleted
    And "new.txt" is created with 4 lines and staged
    And "scratch.txt" is created and not staged
    Then the changes list of "alpha" is:
      | file        | change    | added | removed |
      | edit.txt    | Modified  | 2     | 1       |
      | gone.txt    | Deleted   | 0     | 2       |
      | new.txt     | Added     | 4     | 0       |
      | scratch.txt | Untracked |       |         |

  @core
  Scenario: Comparing with the last commit and with the branch point
    Given a repository "alpha" where "notes.txt" reads "base"
    And on a new branch "feature" "notes.txt" is committed as "feature work"
    And "main" has since moved on with "notes.txt" reading "main moved on"
    When "notes.txt" is edited to "uncommitted" on "feature"
    Then against the last commit "notes.txt" was "feature work"
    And against the branch point with "main" "notes.txt" was "base"

  @core
  Scenario: Saving an edit writes the file
    Given a repository "alpha" where "notes.txt" reads "one"
    And I opened "notes.txt" in "alpha"
    When I save "notes.txt" as "two"
    Then "notes.txt" in "alpha" reads "two"

  @core
  Scenario: A save never overwrites a file that changed on disk since I opened it
    Given a repository "alpha" where "notes.txt" reads "one"
    And I opened "notes.txt" in "alpha"
    And something else changes "notes.txt" to "theirs"
    When I save "notes.txt" as "mine"
    Then the save is refused because the file changed on disk
    And "notes.txt" in "alpha" reads "theirs"

  @core
  Scenario: Choosing to keep mine after a conflict saves my version
    Given a repository "alpha" where "notes.txt" reads "one"
    And I opened "notes.txt" in "alpha"
    And something else changes "notes.txt" to "theirs"
    When I choose to keep my version "mine" of "notes.txt"
    Then "notes.txt" in "alpha" reads "mine"

  @core
  Scenario Outline: A save never escapes the workspace or writes into the git directory
    Given a repository "alpha" with a junction "linked" to a folder outside it
    When I try to save "<path>" in "alpha"
    Then the save is refused because <reason>
    And nothing was written outside "alpha" or inside its git directory

    Examples:
      | path                  | reason                          |
      | ../outside.txt        | the path is outside the workspace |
      | linked/new.txt        | the path is outside the workspace |
      | /ao-outside-workspace/evil.txt   | the path is outside the workspace |
      | .git/hooks/pre-commit | the path is inside the git directory |
      | .GIT/config           | the path is inside the git directory |

  @core
  Scenario: Staging, committing and pushing
    Given a repository "alpha" with a remote
    And "a.txt" and "b.txt" are changed in "alpha"
    When I stage "a.txt" in "alpha"
    And I commit "alpha" with the message "Change a"
    Then the last commit of "alpha" is "Change a" and changes only "a.txt"
    And "b.txt" is still uncommitted in "alpha"
    When I push "alpha"
    Then the remote has "Change a" on "main"

  @core
  Scenario: A commit needs a message
    Given a repository "alpha"
    And "a.txt" and "b.txt" are changed in "alpha"
    And I stage "a.txt" in "alpha"
    When I commit "alpha" with the message "   "
    Then the commit is refused because the message is empty

  @ui
  Scenario: The changes list shows changed files with line counts
    Given a session open in repository "alpha"
    And "notes.txt" in "alpha" gains 2 lines
    And "fresh.txt" is created in "alpha"
    When I look at the changes in "alpha"
    Then the changes list shows:
      | file      | change    | counts |
      | notes.txt | Modified  | +2     |
      | fresh.txt | Untracked |        |

  @ui
  Scenario: Changes are shown as a tree of folders, or as a flat list
    Given a session open in repository "alpha"
    And "docs/guide.txt" is created in "alpha"
    When I look at the changes in "alpha"
    Then the changes show the "docs" folder with "guide.txt" inside it
    When I switch the changes to a list
    Then the changes show "docs/guide.txt" on one row

  @ui
  Scenario: Editing a file and saving it
    Given a session open in repository "alpha"
    And I am editing "notes.txt" in "alpha"
    When I type "edited in the app" into the editor
    And I save with Ctrl+S
    Then "notes.txt" in "alpha" contains "edited in the app"
    And the editor shows no unsaved changes

  @ui
  Scenario: A file changed on disk while I have no edits reloads
    Given a session open in repository "alpha"
    And I am editing "notes.txt" in "alpha"
    When "notes.txt" in "alpha" is changed on disk to "updated by Claude"
    Then the editor shows "updated by Claude"

  @ui
  Scenario: A file changed on disk while I have unsaved edits asks me and never overwrites silently
    Given a session open in repository "alpha"
    And I am editing "notes.txt" in "alpha"
    And I type "my edit" into the editor
    When "notes.txt" in "alpha" is changed on disk to "updated by Claude"
    Then I am told the file changed on disk
    And "notes.txt" in "alpha" contains "updated by Claude"
    When I choose "Keep mine"
    Then "notes.txt" in "alpha" contains "my edit"

  @ui
  Scenario: Staging a file and committing it
    Given a session open in repository "alpha"
    And "notes.txt" in "alpha" gains 2 lines
    And I look at the changes in "alpha"
    When I stage "notes.txt"
    And I commit with the message "Update notes"
    Then I am told "Committed."
    And the last commit of "alpha" is "Update notes"
