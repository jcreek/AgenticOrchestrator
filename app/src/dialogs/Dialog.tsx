import { useEffect, useRef, type KeyboardEvent, type ReactNode } from "react";
import "./Dialog.css";

interface Props {
  title: string;
  onCancel: () => void;
  footer: ReactNode;
  children: ReactNode;
  onKeyDown?: (event: KeyboardEvent<HTMLDivElement>) => void;
}

const FOCUSABLE = 'button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex="-1"])';

export function Dialog({ title, onCancel, footer, children, onKeyDown }: Props) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    const dialog = ref.current;
    (dialog?.querySelector<HTMLElement>("[data-autofocus]") ?? dialog)?.focus();
    return () => previous?.focus();
  }, []);

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      onCancel();
      return;
    }
    if (event.key === "Tab") {
      const items = [...(ref.current?.querySelectorAll<HTMLElement>(FOCUSABLE) ?? [])];
      if (items.length === 0) return;
      const first = items[0];
      const last = items[items.length - 1];
      const onDialog = document.activeElement === ref.current;
      if (event.shiftKey && (onDialog || document.activeElement === first)) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && (onDialog || document.activeElement === last)) {
        event.preventDefault();
        first.focus();
      }
      return;
    }
    onKeyDown?.(event);
  };

  return (
    <div className="dialog-backdrop">
      <div
        ref={ref}
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-label={title}
        tabIndex={-1}
        onKeyDown={handleKeyDown}
      >
        <h2>{title}</h2>
        <div className="dialog-body">{children}</div>
        <div className="dialog-footer">{footer}</div>
      </div>
    </div>
  );
}
