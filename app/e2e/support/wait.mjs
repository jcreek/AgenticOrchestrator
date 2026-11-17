export const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

export const DEFAULT_WAIT_MS = 20_000;

/**
 * Polls `check` until it returns a truthy value and returns that value. On timeout, fails with
 * `what` and the last value or error seen, so the failure says what was expected and what was there.
 */
export async function until(what, check, timeoutMs = DEFAULT_WAIT_MS) {
  const deadline = Date.now() + timeoutMs;
  let last;
  for (;;) {
    try {
      last = await check();
      if (last) return last;
    } catch (error) {
      last = error;
    }
    if (Date.now() >= deadline) {
      const seen = last instanceof Error ? last.message : JSON.stringify(last);
      throw new Error(`Timed out after ${timeoutMs / 1000}s waiting for ${what}. Last seen: ${seen}`);
    }
    await sleep(100);
  }
}
