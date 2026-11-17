import { STATUS_COLOR, type StatusKind } from "../state/status";

export function StatusIcon({ kind, color }: { kind: StatusKind; color?: string }) {
  const tint = color ?? STATUS_COLOR[kind];
  const common = { width: 14, height: 14, viewBox: "0 0 14 14", "aria-hidden": true, className: "status-icon" };
  switch (kind) {
    case "working":
      return (
        <svg {...common} fill="none" stroke={tint} strokeWidth="2" strokeLinecap="round">
          <path d="M7 1.5A5.5 5.5 0 1 1 1.5 7" />
        </svg>
      );
    case "input":
      return (
        <svg {...common} fill={tint}>
          <path d="M1.5 2h11v8h-6l-3 2.5V10h-2z" />
        </svg>
      );
    case "permission":
      return (
        <svg {...common} fill={tint} fillRule="evenodd">
          <path d="M4 6V4.5a3 3 0 0 1 6 0V6h1v6.5H3V6zm1.5 0h3V4.5a1.5 1.5 0 0 0-3 0z" />
        </svg>
      );
    case "finished":
      return (
        <svg {...common} fill="none" stroke={tint} strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <path d="M2.5 7.5l3 3 6-7" />
        </svg>
      );
    case "crashed":
      return (
        <svg {...common} fill="none" stroke={tint} strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
          <path d="M7 1.8l5.6 10.2H1.4z" />
          <path d="M7 5.8v3M7 10.4v.1" />
        </svg>
      );
    case "idle":
      return (
        <svg {...common} fill="none" stroke={tint} strokeWidth="1.5">
          <circle cx="7" cy="7" r="5" strokeDasharray="2.2 2.2" />
        </svg>
      );
    case "unknown":
      return (
        <svg {...common} fill="none" stroke={tint} strokeWidth="1.5" strokeLinecap="round">
          <circle cx="7" cy="7" r="5.2" />
          <path d="M5.5 5.7a1.6 1.6 0 1 1 2.3 1.5c-.5.3-.8.6-.8 1.1M7 10.2v.1" />
        </svg>
      );
  }
}

/** The warning triangle used by alert banners, in any colour. */
export function WarningIcon({ color }: { color: string }) {
  return <StatusIcon kind="crashed" color={color} />;
}
