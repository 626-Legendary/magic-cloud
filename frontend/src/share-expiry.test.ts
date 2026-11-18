import { describe, expect, it } from "vitest";
import { shareExpiryTimestamp } from "./share-expiry";
describe("share expiry", () => {
  it("uses precise minute, hour and day durations from confirmation time", () => {
    const now = 1700000000999;
    expect(shareExpiryTimestamp(300, now)).toBe(1700000300);
    expect(shareExpiryTimestamp(3600, now)).toBe(1700003600);
    expect(shareExpiryTimestamp(21600, now)).toBe(1700021600);
    expect(shareExpiryTimestamp(43200, now)).toBe(1700043200);
    expect(shareExpiryTimestamp(604800, now)).toBe(1700604800);
    expect(shareExpiryTimestamp(1209600, now)).toBe(1701209600);
    expect(shareExpiryTimestamp(2592000, now)).toBe(1702592000);
  });
  it("represents a permanent link with null and rejects zero or custom values", () => {
    expect(shareExpiryTimestamp(null)).toBeNull();
    for (const value of [0, -1, 1, NaN, Infinity])
      expect(() => shareExpiryTimestamp(value)).toThrow("invalid_expiry");
  });
});
