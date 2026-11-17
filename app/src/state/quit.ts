/** Shared controller for native Quit and window-close confirmation. */
export function createQuitController(options: {
  needsConfirmation: () => boolean;
  showConfirmation: () => void;
  hideConfirmation: () => void;
  close: () => Promise<unknown>;
  onError: (error: unknown) => void;
}) {
  let confirming = false;
  let exiting = false;
  const approve = async () => {
    if (exiting) return;
    exiting = true;
    try {
      await options.close();
    } catch (error) {
      exiting = false;
      options.onError(error);
    }
  };
  return {
    request(event?: { preventDefault: () => void }) {
      if (exiting) { event?.preventDefault(); return; }
      if (options.needsConfirmation()) {
        event?.preventDefault();
        if (!confirming) {
          confirming = true;
          options.showConfirmation();
        }
      } else if (!event) {
        void approve();
      }
    },
    approve,
    cancel() {
      confirming = false;
      options.hideConfirmation();
    },
  };
}
