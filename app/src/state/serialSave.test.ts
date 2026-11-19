import { describe, expect, it } from "vitest";
import { serialSaver } from "./serialSave";

function deferredSaves() {
  const started: string[] = [];
  const finish: (() => void)[] = [];
  const save = (value: string) => {
    started.push(value);
    return new Promise<void>((resolve) => finish.push(resolve));
  };
  return { started, finish, save };
}

const flush = () => new Promise((resolve) => setTimeout(resolve));

describe("serialSaver", () => {
  it("waits for the running save and then writes only the latest value", async () => {
    const { started, finish, save } = deferredSaves();
    const idle: unknown[] = [];
    const queue = serialSaver(save, (error) => idle.push(error));

    queue("a");
    queue("ab");
    queue("abc");
    expect(started).toEqual(["a"]);

    finish[0]();
    await flush();
    expect(started).toEqual(["a", "abc"]);
    expect(idle).toEqual([]);

    finish[1]();
    await flush();
    expect(idle).toEqual([null]);
  });

  it("reports the error when the last save fails, and clears it when a later one succeeds", async () => {
    const idle: unknown[] = [];
    let fail = true;
    const queue = serialSaver(
      () => (fail ? Promise.reject(new Error("disk full")) : Promise.resolve()),
      (error) => idle.push(error),
    );

    queue("a");
    await flush();
    fail = false;
    queue("ab");
    await flush();

    expect(idle).toEqual([new Error("disk full"), null]);
  });
});
