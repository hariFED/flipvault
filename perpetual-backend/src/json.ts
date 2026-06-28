// BigInt-safe JSON helpers. Prisma returns `bigint` for our money columns, and
// JSON.stringify throws on bigint by default — so we serialize bigints as plain
// decimal strings (base units). The API contract: every *Sol/*Token/amount
// field is a string of integer base units (lamports / 6-decimal token units).

export function jsonStringify(value: unknown): string {
  return JSON.stringify(value, (_key, v) => (typeof v === "bigint" ? v.toString() : v));
}

// Express helper: res.json() can't handle bigints, so send pre-serialized text.
import type { Response } from "express";
export function sendJson(res: Response, status: number, body: unknown): void {
  res.status(status).type("application/json").send(jsonStringify(body));
}
