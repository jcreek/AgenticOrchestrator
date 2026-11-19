Feature: Settings
  Settings tell you which version of the app you are running.

  @ui
  Scenario: Settings shows the app version
    Given the app is open with no sessions
    When I open Settings
    Then Settings shows the app version
