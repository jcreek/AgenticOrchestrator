# Performance

What the app does to stay cheap, what was measured, and what is still slow. Figures are medians from one developer machine in November 2026, where a single git process costs 150 to 400 ms. Treat them as relative, not absolute.

## What costs time

Almost all background cost is starting processes: `claude agents --json` for status (about 380 ms a call) and git for tab summaries and the IDE pane. The rules below exist to start fewer of them.

Terminals and the editor run in WebView2, so typing and scrolling are as fast as xterm.js and Monaco are. Bulk terminal output is capped by Windows ConPTY at about 1 to 2 MB/s.

## Scheduling rules

### Status poller (backend)

- Polls every 1.5 s while any session is starting or working, and for 6 s after a wake.
- A wake is: output from a Claude terminal, a terminal exiting, the viewed tab changing, or the query starting to fail.
- Otherwise it waits up to 15 s between polls.
- It keeps running while the window is minimised, so notifications still arrive.

### Tab summaries (front end, `app/src/state/useSummaries.ts`)

A summary is the branch and changed-file count on a tab and in the overview.

- At most 3 run at once, or 6 while any workspace still has no summary. The rest queue, with the open tab at the front.
- The open tab refreshes when opened, then within 5 s of a change in its folder, and every 30 s regardless. The IDE pane's changes list follows the same rule on a 3 s check.
- Any tab refreshes straight away when its status changes or when it is added.
- When the window gains focus or is restored, summaries older than 15 s refresh.
- The backend watches each workspace folder and keeps a change counter for it (`crates/core/src/watch.rs`). Every 5 s the front end reads the counters, which starts no process, and refreshes background tabs whose counter moved, each at most once per 15 s. A folder that cannot be watched is polled as before.
- As a safety net, a background tab also refreshes when its summary is 5 minutes old.
- Nothing runs while the window is minimised or hidden.

So an idle background tab costs nothing, and a change in its folder shows within a few seconds: measured over an idle minute, the open tab made 12 summary calls and two background tabs made none, and a file written into a background workspace started a refresh 3.2 s later. A folder that changes constantly, such as one being built, refreshes every 15 s.

A watch does not see a commit or branch switch made outside the folder's own files, which happens in a linked worktree because its git data lives in the main repository. The status change, the focus rule or the 5-minute net picks that up.

### Minimised window

The backend hides the WebView2 page while the window is minimised. Rendering stops, timers are throttled, and every front-end poll that checks page visibility stops. Terminal output keeps buffering and status events keep arriving; both are current as soon as the window is restored.

### Terminal output

The first chunk goes to the UI at once. Later chunks are batched for up to 8 ms or 64 KB, and each terminal keeps 2 MB of scrollback for re-attach.

## Measured

### Idle, 20 workspaces, per minute

| State | App CPU (s) | Renderer CPU (s) | Git processes |
|---|---|---|---|
| Session tab open | 2.95 | 0.67 | 160 |
| Overview | 1.36 | 0.22 | 76 |
| Minimised | 0.20 | 0.02 | 4 |

Before the scheduling work the app process used about 10 to 11 CPU seconds and started about 500 git processes a minute in all three states.

This table was measured when every tab and the IDE pane refreshed on timers. They now refresh on a change, so the session-tab and overview rows overstate the current cost; they have not been measured again.

### Actions

| Action | Time |
|---|---|
| Window appears after launch | about 185 ms |
| Launcher start when already built | about 0.5 s |
| Key press to echo in an idle terminal | about 0.6 ms in the backend |
| Switch tab | about 37 ms |
| Open a tab's IDE pane: changes list painted | about 670 ms |
| Open a tab's IDE pane: first diff painted | about 1.0 s |
| First tab summary after start, 20 workspaces | about 2.0 s |
| All 20 summaries, from page load | about 3 s |
| Remove a worktree | about 1 s |

Memory: the app process uses about 30 MB; WebView2 uses about 330 MB.

## Known slow spots

- **Opening the IDE pane** is bounded by `git status` itself. Its three git processes (`rev-parse`, `status`, `diff --numstat`) run side by side, which took the core call from 471 to 340 ms on a 3,000-file repository; the rest is git scanning the folder. The open-pane figures above were measured before that change.
- **Start-up fill** of 20 summaries takes about 3 s after the page loads, 6 at a time. Limits of 3, 6, 10 and 20 were measured: 3 took about 4.6 s; 10 and 20 delayed the first summary (0.9 s at 6, 1.7 s at 20) without finishing sooner. Tabs show the folder name until their summary arrives.
- **Monaco** is a 3.9 MB chunk loaded the first time an IDE pane opens.

## Measuring again

Backend benchmarks, which print their own figures:

```
cargo test --release -p orchestrator-core --test perf -- --ignored --nocapture --test-threads=1
```

The front-end figures came from driving a release build over WebView2 remote debugging with 20 temporary repositories. Those scripts are not in the repo.
