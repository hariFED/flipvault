// ─────────────────────────────────────────────────────────────────────────
// OHLCV candle aggregation — SCAFFOLDING ONLY (§10).
//
// The `Candle` Prisma model exists now, but live aggregation is intentionally
// left off for the current scope. The intended approach is documented below so
// it can be switched on later. Every trade path (deposit autobuy, pick
// liquidation, manual) calls `feedCandle(...)` so the wiring is already in place
// — the function is a no-op placeholder today.
// ─────────────────────────────────────────────────────────────────────────

export const TIMEFRAMES = ["1m", "5m", "15m", "1h", "4h", "1d"] as const;
export type Timeframe = (typeof TIMEFRAMES)[number];

export const TIMEFRAME_SECONDS: Record<Timeframe, number> = {
  "1m": 60,
  "5m": 300,
  "15m": 900,
  "1h": 3600,
  "4h": 14400,
  "1d": 86400,
};

export interface TradePoint {
  mint: string;
  price: string; // spot price after the trade (lamports per token base unit)
  volumeSol: bigint;
  volumeToken: bigint;
  at: Date;
}

// Called on every trade. NO-OP for now (§10).
export async function feedCandle(_point: TradePoint): Promise<void> {
  // TODO: implement OHLCV aggregation.
  //
  // For each timeframe tf in TIMEFRAMES:
  //   1. bucket = floor(point.at / TIMEFRAME_SECONDS[tf]) * TIMEFRAME_SECONDS[tf]
  //   2. read the in-progress candle from Valkey: candle:{mint}:{tf}:current
  //   3. if none, or its openTime != bucket:
  //        - flush the previous in-progress candle (if any) to Postgres
  //          (upsert on the unique [tokenMint, timeframe, openTime])
  //        - open a new candle: open = high = low = close = point.price,
  //          volumeSol = volumeToken = 0, tradeCount = 0
  //   4. update the current candle:
  //        - high = max(high, price); low = min(low, price); close = price
  //        - volumeSol += point.volumeSol; volumeToken += point.volumeToken
  //        - tradeCount += 1
  //   5. write the updated current candle back to Valkey.
  //
  // A background sweeper (or lazy flush on first trade of the next bucket)
  // moves closed candles from Valkey to Postgres.
}

// Called by GET /candles. Returns closed candles only for now (§10).
export async function getCandles(
  _mint: string,
  _timeframe: Timeframe
): Promise<unknown[]> {
  // TODO: implement OHLCV aggregation.
  // Read closed candles from Postgres (prisma.candle.findMany ordered by
  // openTime), then merge the in-progress candle from Valkey
  // (candle:{mint}:{tf}:current) as the most recent bar.
  return [];
}
