import type { AppError, ErrorKind } from "../bindings";

export function isAppError(error: unknown): error is AppError {
  return typeof error === "object" && error !== null && "kind" in error && "message" in error;
}

export function errorKind(error: unknown): ErrorKind | null {
  return isAppError(error) ? error.kind : null;
}

export function errorMessage(error: unknown): string {
  if (isAppError(error)) return error.message;
  if (error instanceof Error) return error.message;
  return String(error);
}
