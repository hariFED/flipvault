// ─────────────────────────────────────────────────────────────────────────
// Valkey (Redis-compatible) hot read path + distributed lock (§5, §9).
//
// Postgres is the source of truth; Valkey is the cache. Strategy = write-through:
// every state change updates Postgres inside a transaction, then refreshes the
// Valkey cache. On a cache miss we rebuild from Postgres.
//
// Key layout (§5):
//   curve:{mint}        hash   vSol vTok rSol rTok feeBps price complete
//   user:{wallet}       hash   solBalance tokenBalance status
//   trades:{mint}       stream  recent buys/sells feed
//   lock:curve:{mint}   string  distributed lock token (SET NX PX)
// ─────────────────────────────────────────────────────────────────────────
import { randomUUID } from "node:crypto";
import Redis from "ioredis";

export const valkey = new Redis(process.env.VALKEY_URL ?? "redis://localhost:6379");

valkey.on("connect", () => console.log("✓ connected to Valkey"));
valkey.on("error", (e) => console.error("Valkey error:", e.message));

// ── Key builders ─────────────────────────────────────────────────────────
export const keys = {
  curve: (mint: string) => `curve:${mint}`,
  user: (wallet: string) => `user:${wallet}`,
  trades: (mint: string) => `trades:${mint}`,
  lock: (mint: string) => `lock:curve:${mint}`,
  candleCurrent: (mint: string, tf: string) => `candle:${mint}:${tf}:current`,
};

// ── Generic cache-aside helper (used for read endpoints) ───────────────────
export async function cached<T>(
  key: string,
  ttlSeconds: number,
  loader: () => Promise<T>
): Promise<{ value: T; hit: boolean }> {
  const found = await valkey.get(key);
  if (found !== null) return { value: JSON.parse(found) as T, hit: true };
  const value = await loader();
  await valkey.set(key, JSON.stringify(value), "EX", ttlSeconds);
  return { value, hit: false };
}

// ── Curve hash cache (the hot value reads derive withdrawable from) ─────────
export interface CurveCache {
  vSol: bigint;
  vTok: bigint;
  rSol: bigint;
  rTok: bigint;
  feeBps: number;
  price: string;
  complete: boolean;
}

export async function writeCurveCache(mint: string, c: CurveCache): Promise<void> {
  await valkey.hset(keys.curve(mint), {
    vSol: c.vSol.toString(),
    vTok: c.vTok.toString(),
    rSol: c.rSol.toString(),
    rTok: c.rTok.toString(),
    feeBps: c.feeBps.toString(),
    price: c.price,
    complete: c.complete ? "1" : "0",
  });
}

export async function readCurveCache(mint: string): Promise<CurveCache | null> {
  const h = await valkey.hgetall(keys.curve(mint));
  if (!h || Object.keys(h).length === 0) return null;
  return {
    vSol: BigInt(h.vSol),
    vTok: BigInt(h.vTok),
    rSol: BigInt(h.rSol),
    rTok: BigInt(h.rTok),
    feeBps: Number(h.feeBps),
    price: h.price,
    complete: h.complete === "1",
  };
}

// ── User hash cache (write-through mirror of the user row) ──────────────────
export interface UserCache {
  solBalance: bigint;
  tokenBalance: bigint;
  status: string;
}

export async function writeUserCache(wallet: string, u: UserCache): Promise<void> {
  await valkey.hset(keys.user(wallet), {
    solBalance: u.solBalance.toString(),
    tokenBalance: u.tokenBalance.toString(),
    status: u.status,
  });
}

export async function readUserCache(wallet: string): Promise<UserCache | null> {
  const h = await valkey.hgetall(keys.user(wallet));
  if (!h || Object.keys(h).length === 0) return null;
  return {
    solBalance: BigInt(h.solBalance),
    tokenBalance: BigInt(h.tokenBalance),
    status: h.status,
  };
}

// ── Recent-trades feed (Valkey stream, capped) ──────────────────────────────
export interface TradeFeedEntry {
  side: string;
  solAmount: string;
  tokenAmount: string;
  price: string;
  source: string;
  wallet: string;
}

export async function pushTrade(mint: string, t: TradeFeedEntry): Promise<void> {
  // MAXLEN ~ keeps the stream bounded (approximate trimming for speed).
  await valkey.xadd(
    keys.trades(mint),
    "MAXLEN",
    "~",
    "1000",
    "*",
    "side",
    t.side,
    "solAmount",
    t.solAmount,
    "tokenAmount",
    t.tokenAmount,
    "price",
    t.price,
    "source",
    t.source,
    "wallet",
    t.wallet
  );
}

export async function readRecentTrades(mint: string, count = 50): Promise<TradeFeedEntry[]> {
  // XREVRANGE returns newest-first.
  const rows = await valkey.xrevrange(keys.trades(mint), "+", "-", "COUNT", count);
  return rows.map(([, fields]) => {
    const obj: Record<string, string> = {};
    for (let i = 0; i < fields.length; i += 2) obj[fields[i]] = fields[i + 1];
    return {
      side: obj.side,
      solAmount: obj.solAmount,
      tokenAmount: obj.tokenAmount,
      price: obj.price,
      source: obj.source,
      wallet: obj.wallet,
    };
  });
}

// ── Distributed lock (§9): SET NX PX + safe compare-and-delete release ───────
const RELEASE_LUA =
  "if redis.call('get', KEYS[1]) == ARGV[1] then return redis.call('del', KEYS[1]) else return 0 end";

export class LockError extends Error {}

// Acquire lock:curve:{mint}. Retries briefly so concurrent trades serialize
// rather than failing outright. Returns a release() function.
export async function acquireCurveLock(
  mint: string,
  opts: { ttlMs?: number; retries?: number; retryDelayMs?: number } = {}
): Promise<() => Promise<void>> {
  const { ttlMs = 5000, retries = 50, retryDelayMs = 50 } = opts;
  const key = keys.lock(mint);
  const token = randomUUID();

  for (let attempt = 0; attempt <= retries; attempt++) {
    const ok = await valkey.set(key, token, "PX", ttlMs, "NX");
    if (ok === "OK") {
      return async () => {
        await valkey.eval(RELEASE_LUA, 1, key, token);
      };
    }
    await new Promise((r) => setTimeout(r, retryDelayMs));
  }
  throw new LockError(`could not acquire curve lock for ${mint}`);
}
