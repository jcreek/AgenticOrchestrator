import type { ReactNode } from "react";

export interface Action {
  label: string;
  onClick: () => void;
  primary?: boolean;
  disabled?: boolean;
}

function Actions({ actions }: { actions: Action[] }) {
  return actions.map((action) => (
    <button
      key={action.label}
      type="button"
      className={action.primary ? "ide-action ide-action--primary" : "ide-action"}
      onClick={action.onClick}
      disabled={action.disabled}
    >
      {action.label}
    </button>
  ));
}

/** Centred state for an empty, large, binary or failed body. */
export function Message({ title, detail, actions = [], alert = false }: {
  title: string;
  detail?: ReactNode;
  actions?: Action[];
  alert?: boolean;
}) {
  return (
    <div className="ide-message" role={alert ? "alert" : undefined}>
      <span className="ide-message-title">{title}</span>
      {detail && <span className="ide-message-detail">{detail}</span>}
      {actions.length > 0 && (
        <span className="ide-message-actions">
          <Actions actions={actions} />
        </span>
      )}
    </div>
  );
}

/** Full-width alert strip as drawn in the error states design. */
export function Banner({ title, detail, actions = [], tone }: {
  title: string;
  detail: ReactNode;
  actions?: Action[];
  tone: "error" | "neutral";
}) {
  return (
    <div className={`ide-banner ide-banner--${tone}`} role="alert">
      <svg width="14" height="14" viewBox="0 0 14 14" fill="none" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d="M7 1.8l5.6 10.2H1.4z" />
        <path d="M7 5.8v3M7 10.4v.1" />
      </svg>
      <span className="ide-banner-text">
        <span className="ide-banner-title">{title}</span>
        <span className="ide-banner-detail">{detail}</span>
      </span>
      <Actions actions={actions} />
    </div>
  );
}
