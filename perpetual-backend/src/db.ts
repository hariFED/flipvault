// One shared PrismaClient for the whole app.
// Create it ONCE and reuse it — it holds a connection pool, so making a new
// one per-request would exhaust Postgres connections.
//
// Prisma 7 dropped the old Rust query engine: the client now talks to Postgres
// through a "driver adapter" — here, @prisma/adapter-pg (the node-postgres driver).
import "dotenv/config";
import { PrismaClient } from "@prisma/client";
import { PrismaPg } from "@prisma/adapter-pg";

const adapter = new PrismaPg({ connectionString: process.env.DATABASE_URL });

export const prisma = new PrismaClient({
  adapter,
  log: ["warn", "error"], // add "query" to see every SQL statement Prisma runs
});
