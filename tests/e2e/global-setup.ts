import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createInterface } from "node:readline";

import type { FullConfig } from "@playwright/test";

interface ReadyOrigins {
  editorOrigin: string;
  previewOrigin: string;
}

const run = async (command: string, args: string[]): Promise<void> => {
  await new Promise<void>((resolveRun, rejectRun) => {
    const child = spawn(command, args, {
      cwd: process.cwd(),
      env: process.env,
      stdio: "inherit",
    });
    child.once("error", rejectRun);
    child.once("exit", (code, signal) => {
      if (code === 0) {
        resolveRun();
      } else {
        rejectRun(
          new Error(
            `${command} exited before E2E startup (code=${String(code)}, signal=${String(signal)})`,
          ),
        );
      }
    });
  });
};

const parseReadyOrigins = (line: string): ReadyOrigins | null => {
  const prefix = "LP_STUDIO_READY ";
  if (!line.startsWith(prefix)) return null;
  try {
    const value = JSON.parse(line.slice(prefix.length)) as Record<
      string,
      unknown
    >;
    if (
      typeof value.editorOrigin !== "string" ||
      typeof value.previewOrigin !== "string"
    ) {
      return null;
    }
    const editor = new URL(value.editorOrigin);
    const preview = new URL(value.previewOrigin);
    if (
      editor.protocol !== "http:" ||
      preview.protocol !== "http:" ||
      editor.hostname !== "127.0.0.1" ||
      preview.hostname !== "127.0.0.1" ||
      editor.origin === preview.origin
    ) {
      return null;
    }
    return {
      editorOrigin: editor.origin,
      previewOrigin: preview.origin,
    };
  } catch {
    return null;
  }
};

const waitForReadyLine = async (
  server: ChildProcessWithoutNullStreams,
): Promise<ReadyOrigins> =>
  new Promise<ReadyOrigins>((resolveReady, rejectReady) => {
    const lines = createInterface({ input: server.stdout });
    const timeout = setTimeout(() => {
      lines.close();
      rejectReady(new Error("Local server did not publish LP_STUDIO_READY"));
    }, 60_000);
    const finish = (callback: () => void) => {
      clearTimeout(timeout);
      lines.close();
      callback();
    };
    lines.on("line", (line) => {
      const origins = parseReadyOrigins(line);
      if (origins !== null) finish(() => resolveReady(origins));
    });
    server.once("error", (error) => finish(() => rejectReady(error)));
    server.once("exit", (code, signal) => {
      finish(() =>
        rejectReady(
          new Error(
            `Local server exited before readiness (code=${String(code)}, signal=${String(signal)})`,
          ),
        ),
      );
    });
  });

const waitForHealth = async (origin: string): Promise<void> => {
  const deadline = Date.now() + 15_000;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(`${origin}/health`, {
        signal: AbortSignal.timeout(1_000),
      });
      if (response.ok) return;
    } catch {
      // The listener may not yet have entered its accept loop.
    }
    await new Promise((resolveWait) => setTimeout(resolveWait, 50));
  }
  throw new Error(`Local server health check timed out for ${origin}`);
};

const stopServer = async (
  server: ChildProcessWithoutNullStreams,
): Promise<void> => {
  if (server.exitCode !== null || server.signalCode !== null) return;
  const exited = new Promise<void>((resolveExit) => {
    server.once("exit", () => resolveExit());
  });
  server.kill("SIGINT");
  await Promise.race([
    exited,
    new Promise<void>((resolveTimeout) => {
      setTimeout(resolveTimeout, 5_000);
    }),
  ]);
  if (server.exitCode === null && server.signalCode === null) {
    server.kill("SIGKILL");
    await exited;
  }
};

export default async function globalSetup(
  _config: FullConfig,
): Promise<() => Promise<void>> {
  await run("pnpm", ["--filter", "@synapsegit-lp/web", "build"]);
  await run("cargo", ["build", "-p", "synapsegit-lp-local-server", "--locked"]);

  const stateRoot = await mkdtemp(join(tmpdir(), "synapsegit-lp-studio-e2e-"));
  const server = spawn(resolve("target/debug/synapsegit-lp-local-server"), [], {
    cwd: process.cwd(),
    env: {
      ...process.env,
      LP_STUDIO_EDITOR_PORT: "0",
      LP_STUDIO_PREVIEW_PORT: "0",
      LP_STUDIO_STATE_ROOT: stateRoot,
      LP_STUDIO_WEB_DIST: resolve("apps/web/dist"),
    },
    stdio: ["pipe", "pipe", "pipe"],
  });
  let stderr = "";
  server.stderr.setEncoding("utf8");
  server.stderr.on("data", (chunk: string) => {
    stderr = `${stderr}${chunk}`.slice(-8_192);
  });

  try {
    const origins = await waitForReadyLine(server);
    await Promise.all([
      waitForHealth(origins.editorOrigin),
      waitForHealth(origins.previewOrigin),
    ]);
    process.env.LP_STUDIO_E2E_EDITOR_ORIGIN = origins.editorOrigin;
    process.env.LP_STUDIO_E2E_PREVIEW_ORIGIN = origins.previewOrigin;
  } catch (error) {
    await stopServer(server);
    await rm(stateRoot, { recursive: true, force: true });
    const detail = stderr.trim();
    throw new Error(
      detail.length === 0
        ? String(error)
        : `${String(error)}\nLocal server stderr:\n${detail}`,
    );
  }

  return async () => {
    await stopServer(server);
    await rm(stateRoot, { recursive: true, force: true });
    delete process.env.LP_STUDIO_E2E_EDITOR_ORIGIN;
    delete process.env.LP_STUDIO_E2E_PREVIEW_ORIGIN;
  };
}
