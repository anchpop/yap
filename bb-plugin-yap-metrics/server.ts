import { execFile } from "node:child_process";
import { dirname, resolve } from "node:path";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";

import { defineRpcContract, type BbPluginApi } from "@get-bb/plugin-sdk";
import { z } from "zod";

const execFileAsync = promisify(execFile);

const metricsSchema = z.object({
  weeklyActiveUsers: z.number().int().nonnegative(),
  signupsPastMonth: z.number().int().nonnegative(),
  generatedAt: z.string(),
  activityWindowStart: z.string(),
  signupWindowStart: z.string(),
});

export type Metrics = z.infer<typeof metricsSchema>;

export const rpcContract = defineRpcContract({
  getMetrics: {
    input: z.null(),
    output: metricsSchema,
  },
});

function repositoryRoot(): string {
  const sourceDirectory = dirname(fileURLToPath(import.meta.url));
  const pluginDirectory =
    sourceDirectory.endsWith("/dist") ? dirname(sourceDirectory) : sourceDirectory;
  return resolve(pluginDirectory, "..");
}

export function parseMetricsOutput(output: string): Metrics {
  let parsed: unknown;
  try {
    parsed = JSON.parse(output.trim());
  } catch {
    throw new Error("The local metrics helper returned invalid output");
  }

  const result = metricsSchema.safeParse(parsed);
  if (!result.success) {
    throw new Error("The local metrics helper returned invalid output");
  }
  return result.data;
}

async function runLocalMetricsHelper(): Promise<Metrics> {
  try {
    const { stdout } = await execFileAsync(
      "cargo",
      ["run", "--quiet", "-p", "user-metrics", "--", "--summary-json"],
      {
        cwd: repositoryRoot(),
        encoding: "utf8",
        maxBuffer: 1_048_576,
        timeout: 120_000,
      },
    );
    return parseMetricsOutput(stdout);
  } catch (cause) {
    if (cause instanceof Error && cause.message.startsWith("The local metrics helper")) {
      throw cause;
    }
    throw new Error(
      "The local Yap metrics helper failed. Check the repository .env and run it once from the terminal.",
    );
  }
}

export default function plugin(bb: BbPluginApi) {
  bb.rpc.register(rpcContract, {
    getMetrics: runLocalMetricsHelper,
  });
}
