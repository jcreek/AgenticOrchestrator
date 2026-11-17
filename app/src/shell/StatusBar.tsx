import type { ReactNode } from "react";

export function StatusBar({ children, hints }: { children: ReactNode; hints: string }) {
  return (
    <footer className="status-bar">
      {children}
      <span className="status-bar-hints">{hints}</span>
    </footer>
  );
}

export interface Counts {
  needYou: number;
  working: number;
  finished: number;
  idle: number;
}

export function SessionCounts({ counts }: { counts: Counts }) {
  return (
    <>
      {counts.needYou > 0 && <span className="status-bar-need">{counts.needYou} need you</span>}
      {counts.working > 0 && <span>{counts.working} working</span>}
      {counts.finished > 0 && <span>{counts.finished} finished</span>}
      {counts.idle > 0 && <span>{counts.idle} idle</span>}
    </>
  );
}
