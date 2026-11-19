/**
 * Saves values one at a time, so an older save can never land after a newer one. Values queued
 * while a save runs collapse to the latest. `onIdle` gets the last save's error, or null.
 */
export function serialSaver<T>(save: (value: T) => Promise<unknown>, onIdle: (error: unknown) => void): (value: T) => void {
  let pending: { value: T } | null = null;
  let running = false;

  const run = async () => {
    running = true;
    let error: unknown = null;
    while (pending) {
      const { value } = pending;
      pending = null;
      try {
        await save(value);
        error = null;
      } catch (caught) {
        error = caught;
      }
    }
    running = false;
    onIdle(error);
  };

  return (value) => {
    pending = { value };
    if (!running) void run();
  };
}
