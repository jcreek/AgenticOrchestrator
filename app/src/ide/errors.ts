import type { AppError } from "../bindings";

export function toAppError(error: unknown): AppError {
  if (error && typeof error === "object" && "kind" in error && "message" in error) return error as AppError;
  if (error instanceof Error) return { kind: "internal", message: error.message };
  return { kind: "internal", message: String(error) };
}

export function describeError(error: AppError): { title: string; detail: string } {
  switch (error.kind) {
    case "tooLarge":
      return { title: "File too large to show", detail: "Files over 5 MB open in VS Code instead." };
    case "notText":
      return { title: "Binary file", detail: "This file is not text, so it is not shown here." };
    case "notFound":
      return { title: "File not found", detail: "It was deleted or moved." };
    case "outsideWorkspace":
      return { title: "Outside the workspace", detail: error.message };
    case "timedOut":
      return { title: "Git timed out", detail: error.message };
    case "git":
      return { title: "Git failed", detail: error.message };
    default:
      return { title: "Something went wrong", detail: error.message };
  }
}
