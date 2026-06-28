-- CreateEnum
CREATE TYPE "UserStatus" AS ENUM ('ACTIVE', 'INACTIVE');

-- CreateEnum
CREATE TYPE "TradeSide" AS ENUM ('BUY', 'SELL');

-- CreateEnum
CREATE TYPE "TradeSource" AS ENUM ('DEPOSIT_AUTOBUY', 'PICK_LIQUIDATION', 'MANUAL');

-- CreateEnum
CREATE TYPE "DepositStatus" AS ENUM ('PENDING', 'CONFIRMED', 'FAILED');

-- CreateEnum
CREATE TYPE "WithdrawalStatus" AS ENUM ('PENDING', 'CONFIRMED', 'FAILED');

-- CreateTable
CREATE TABLE "users" (
    "id" TEXT NOT NULL,
    "walletAddress" TEXT NOT NULL,
    "privyUserId" TEXT,
    "solBalance" BIGINT NOT NULL DEFAULT 0,
    "tokenBalance" BIGINT NOT NULL DEFAULT 0,
    "status" "UserStatus" NOT NULL DEFAULT 'ACTIVE',
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" TIMESTAMP(3) NOT NULL,

    CONSTRAINT "users_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "bonding_curve_state" (
    "id" TEXT NOT NULL,
    "tokenMint" TEXT NOT NULL,
    "virtualSolReserves" BIGINT NOT NULL,
    "virtualTokenReserves" BIGINT NOT NULL,
    "realSolReserves" BIGINT NOT NULL,
    "realTokenReserves" BIGINT NOT NULL,
    "totalSupply" BIGINT NOT NULL,
    "feeBps" INTEGER NOT NULL DEFAULT 100,
    "lastPrice" DECIMAL(40,18) NOT NULL DEFAULT 0,
    "complete" BOOLEAN NOT NULL DEFAULT false,
    "updatedAt" TIMESTAMP(3) NOT NULL,

    CONSTRAINT "bonding_curve_state_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "trades" (
    "id" TEXT NOT NULL,
    "walletAddress" TEXT NOT NULL,
    "userId" TEXT,
    "tokenMint" TEXT NOT NULL,
    "side" "TradeSide" NOT NULL,
    "solAmount" BIGINT NOT NULL,
    "tokenAmount" BIGINT NOT NULL,
    "price" DECIMAL(40,18) NOT NULL,
    "feePaid" BIGINT NOT NULL,
    "curveSolAfter" BIGINT NOT NULL,
    "curveTokenAfter" BIGINT NOT NULL,
    "source" "TradeSource" NOT NULL DEFAULT 'MANUAL',
    "txSignature" TEXT,
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "trades_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "deposits" (
    "id" TEXT NOT NULL,
    "walletAddress" TEXT NOT NULL,
    "userId" TEXT,
    "amountSol" BIGINT NOT NULL,
    "txSignature" TEXT NOT NULL,
    "status" "DepositStatus" NOT NULL DEFAULT 'CONFIRMED',
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "deposits_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "withdrawals" (
    "id" TEXT NOT NULL,
    "walletAddress" TEXT NOT NULL,
    "userId" TEXT,
    "amountSol" BIGINT NOT NULL,
    "txSignature" TEXT,
    "status" "WithdrawalStatus" NOT NULL DEFAULT 'PENDING',
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "withdrawals_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "candles" (
    "id" TEXT NOT NULL,
    "tokenMint" TEXT NOT NULL,
    "timeframe" TEXT NOT NULL,
    "openTime" TIMESTAMP(3) NOT NULL,
    "open" DECIMAL(40,18) NOT NULL,
    "high" DECIMAL(40,18) NOT NULL,
    "low" DECIMAL(40,18) NOT NULL,
    "close" DECIMAL(40,18) NOT NULL,
    "volumeSol" BIGINT NOT NULL DEFAULT 0,
    "volumeToken" BIGINT NOT NULL DEFAULT 0,
    "tradeCount" INTEGER NOT NULL DEFAULT 0,

    CONSTRAINT "candles_pkey" PRIMARY KEY ("id")
);

-- CreateIndex
CREATE UNIQUE INDEX "users_walletAddress_key" ON "users"("walletAddress");

-- CreateIndex
CREATE UNIQUE INDEX "users_privyUserId_key" ON "users"("privyUserId");

-- CreateIndex
CREATE INDEX "users_status_idx" ON "users"("status");

-- CreateIndex
CREATE UNIQUE INDEX "bonding_curve_state_tokenMint_key" ON "bonding_curve_state"("tokenMint");

-- CreateIndex
CREATE INDEX "trades_tokenMint_createdAt_idx" ON "trades"("tokenMint", "createdAt");

-- CreateIndex
CREATE INDEX "trades_walletAddress_createdAt_idx" ON "trades"("walletAddress", "createdAt");

-- CreateIndex
CREATE UNIQUE INDEX "deposits_txSignature_key" ON "deposits"("txSignature");

-- CreateIndex
CREATE INDEX "deposits_walletAddress_createdAt_idx" ON "deposits"("walletAddress", "createdAt");

-- CreateIndex
CREATE INDEX "withdrawals_walletAddress_createdAt_idx" ON "withdrawals"("walletAddress", "createdAt");

-- CreateIndex
CREATE INDEX "candles_tokenMint_timeframe_openTime_idx" ON "candles"("tokenMint", "timeframe", "openTime");

-- CreateIndex
CREATE UNIQUE INDEX "candles_tokenMint_timeframe_openTime_key" ON "candles"("tokenMint", "timeframe", "openTime");

-- AddForeignKey
ALTER TABLE "trades" ADD CONSTRAINT "trades_userId_fkey" FOREIGN KEY ("userId") REFERENCES "users"("id") ON DELETE SET NULL ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "deposits" ADD CONSTRAINT "deposits_userId_fkey" FOREIGN KEY ("userId") REFERENCES "users"("id") ON DELETE SET NULL ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "withdrawals" ADD CONSTRAINT "withdrawals_userId_fkey" FOREIGN KEY ("userId") REFERENCES "users"("id") ON DELETE SET NULL ON UPDATE CASCADE;
