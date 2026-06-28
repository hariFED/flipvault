// Prisma 7 config. The CLI (migrate, db push, studio, generate) reads this.
// This is where the database connection URL now lives.
import "dotenv/config"; // Prisma 7 no longer auto-loads .env — we do it here
import { defineConfig, env } from "prisma/config";

export default defineConfig({
  schema: "prisma/schema.prisma",
  migrations: {
    path: "prisma/migrations",
  },
  datasource: {
    url: env("DATABASE_URL"),
  },
});
