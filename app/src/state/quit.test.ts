import { expect, it, vi } from "vitest";
import { createQuitController } from "./quit";

function setup(guarded = true) {
  const options = {
    needsConfirmation: vi.fn(() => guarded),
    showConfirmation: vi.fn(),
    hideConfirmation: vi.fn(),
    close: vi.fn(async () => undefined),
    onError: vi.fn(),
  };
  return { ...options, controller: createQuitController(options) };
}

it("coalesces native and window requests; cancel leaves processes and edits alone", () => {
  const test = setup();
  const preventDefault = vi.fn();
  test.controller.request();
  test.controller.request({ preventDefault });
  expect(preventDefault).toHaveBeenCalledOnce();
  expect(test.showConfirmation).toHaveBeenCalledOnce();
  test.controller.cancel();
  expect(test.close).not.toHaveBeenCalled();
  expect(test.hideConfirmation).toHaveBeenCalledOnce();
  test.controller.request();
  expect(test.showConfirmation).toHaveBeenCalledTimes(2);
});

it("approves once without re-opening the confirmation or repeating exit", async () => {
  const test = setup();
  test.controller.request();
  await test.controller.approve();
  test.controller.request();
  await test.controller.approve();
  expect(test.showConfirmation).toHaveBeenCalledOnce();
  expect(test.close).toHaveBeenCalledOnce();
});

it("lets an unguarded window close normally and explicitly approves native Quit", () => {
  const test = setup(false);
  const preventDefault = vi.fn();
  test.controller.request({ preventDefault });
  expect(preventDefault).not.toHaveBeenCalled();
  expect(test.close).not.toHaveBeenCalled();
  test.controller.request();
  expect(test.close).toHaveBeenCalledOnce();
  expect(test.showConfirmation).not.toHaveBeenCalled();
});

it("reports a failed approval and permits retry", async () => {
  const test = setup();
  test.close.mockRejectedValueOnce(new Error("exit failed"));
  await test.controller.approve();
  expect(test.onError).toHaveBeenCalledOnce();
  await test.controller.approve();
  expect(test.close).toHaveBeenCalledTimes(2);
});
