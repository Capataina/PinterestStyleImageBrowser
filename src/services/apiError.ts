/**
 * Backend ApiError discriminated union — wire format from
 * src-tauri/src/commands/error.rs.
 *
 * Audit Inconsistent-Patterns finding: the Tauri commands previously
 * returned `Result<T, String>` and the frontend got back opaque
 * strings ("Failed to fetch image: ..."). The structured kind lets
 * the UI branch — a missing model can trigger a re-download dialog
 * instead of a generic toast.
 *
 * Tauri serialises Rust errors so the catch block sees the JSON
 * object directly (not a string). The shape comes from
 * `#[serde(tag = "kind", content = "details", rename_all = "snake_case")]`.
 */
export type ApiError =
  | { kind: "tokenizer_missing"; details: string }
  | { kind: "text_model_missing"; details: string }
  | { kind: "image_model_missing"; details: string }
  | { kind: "db"; details: string }
  | { kind: "encoder"; details: string }
  | { kind: "cosine"; details: string }
  | { kind: "not_found"; details: string }
  | { kind: "bad_input"; details: string }
  | { kind: "io"; details: string }
  | { kind: "internal"; details: string };

/**
 * True if the value looks like a structured ApiError. Tauri may
 * also throw legacy String errors from commands that haven't
 * migrated yet; this lets the catch site handle both.
 */
export function isApiError(value: unknown): value is ApiError {
  return (
    typeof value === "object" &&
    value !== null &&
    "kind" in value &&
    typeof (value as { kind: unknown }).kind === "string"
  );
}

/**
 * Format an unknown caught value (could be ApiError, string, Error,
 * or anything Tauri threw) into a single human-readable line.
 *
 * Used at every catch boundary so user-facing messages stay
 * consistent. The kind label gives the user an actionable category
 * even when the details are technical — "Tokenizer missing" beats
 * "Failed to fetch images: ...".
 */
export function formatApiError(error: unknown): string {
  if (isApiError(error)) {
    return formatStructured(error);
  }
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

function formatStructured(e: ApiError): string {
  switch (e.kind) {
    case "tokenizer_missing":
      return `Tokenizer file missing at ${e.details}`;
    case "text_model_missing":
      return `Text model missing at ${e.details}`;
    case "image_model_missing":
      return `Image model missing at ${e.details}`;
    case "db":
      return `Database error: ${e.details}`;
    case "encoder":
      return `Encoder error: ${e.details}`;
    case "cosine":
      return `Cosine index error: ${e.details}`;
    case "not_found":
      return `Not found: ${e.details}`;
    case "bad_input":
      return `Invalid input: ${e.details}`;
    case "io":
      return `I/O error: ${e.details}`;
    case "internal":
      return `Internal error: ${e.details}`;
  }
}

/**
 * True if the error indicates a model file the app needs is not on
 * disk. Caller can use this to trigger a re-download flow rather
 * than a generic error toast.
 */
export function isMissingModelError(error: unknown): boolean {
  if (!isApiError(error)) return false;
  return (
    error.kind === "tokenizer_missing" ||
    error.kind === "text_model_missing" ||
    error.kind === "image_model_missing"
  );
}
