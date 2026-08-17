import { describe, expect, it } from "vitest";

import { parseMetricsOutput } from "./server";

describe("parseMetricsOutput", () => {
  it("accepts aggregate-only helper output", () => {
    expect(
      parseMetricsOutput(
        JSON.stringify({
          weeklyActiveUsers: 12,
          signupsPastMonth: 34,
          generatedAt: "2026-08-16T12:00:00Z",
          activityWindowStart: "2026-08-09T12:00:00Z",
          signupWindowStart: "2026-07-17T12:00:00Z",
        }),
      ),
    ).toEqual({
      weeklyActiveUsers: 12,
      signupsPastMonth: 34,
      generatedAt: "2026-08-16T12:00:00Z",
      activityWindowStart: "2026-08-09T12:00:00Z",
      signupWindowStart: "2026-07-17T12:00:00Z",
    });
  });

  it("rejects detailed or malformed helper output", () => {
    expect(() => parseMetricsOutput("user@example.com")).toThrow(
      "The local metrics helper returned invalid output",
    );
  });
});
