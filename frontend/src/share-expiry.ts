export const shareExpiryOptions = [
  { key: "expiry5Minutes", seconds: 5 * 60 },
  { key: "expiry1Hour", seconds: 60 * 60 },
  { key: "expiry6Hours", seconds: 6 * 60 * 60 },
  { key: "expiry12Hours", seconds: 12 * 60 * 60 },
  { key: "expiry7Days", seconds: 7 * 86400 },
  { key: "expiry14Days", seconds: 14 * 86400 },
  { key: "expiry30Days", seconds: 30 * 86400 },
  { key: "permanent", seconds: null },
] as const;
export function shareExpiryTimestamp(seconds: number | null, now = Date.now()) {
  if (!shareExpiryOptions.some((option) => option.seconds === seconds))
    throw new Error("invalid_expiry");
  return seconds === null ? null : Math.floor(now / 1000) + seconds;
}
