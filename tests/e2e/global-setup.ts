import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { createHash } from "node:crypto";
import {
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createInterface } from "node:readline";

import type { FullConfig } from "@playwright/test";

interface ReadyOrigins {
  editorOrigin: string;
  previewOrigin: string;
}

const importFixture = new Map<string, string>([
  [
    "index.html",
    '<!doctype html><html lang="ja"><head><meta charset="utf-8"><link rel="stylesheet" href="assets/theme.css"><script src="assets/app.js"></script><script>window.__LP_STUDIO_INLINE_SCRIPT_MUST_NOT_RUN__=true</script><title>Imported E2E LP</title></head><body><main><h1 data-lp-id="hero-heading">登録ルートから始めるLP</h1><p data-lp-id="hero-copy">コピーされたAccepted状態です。</p><a data-lp-id="hero-cta" href="#contact">相談する</a><a href="about.html">同じLP内の詳細へ</a><a href="docs/">同じLP内のディレクトリ詳細へ</a><img src="assets/missing.png" alt=""><img src="https://blocked.invalid/e2e-preview-canary.png" alt=""></main></body></html>\n',
  ],
  [
    "about.html",
    '<!doctype html><html lang="ja"><head><meta charset="utf-8"><title>Imported detail</title></head><body><main><h1>同じLP内の詳細ページ</h1><a href="./">トップへ戻る</a></main></body></html>\n',
  ],
  [
    "docs/index.html",
    '<!doctype html><html lang="ja"><head><meta charset="utf-8"><title>Imported directory detail</title></head><body><main><h1 data-lp-id="nested-heading">同じLP内のディレクトリ詳細ページ</h1><a href="../">トップへ戻る</a></main></body></html>\n',
  ],
  [
    "assets/app.js",
    'document.documentElement.dataset.selfScript = "executed"; Promise.reject(new Error("E2E isolated rejection"));\n',
  ],
  [
    "assets/theme.css",
    "body { margin: 0; font-family: sans-serif; background: #f5f7ef; color: #182015; }\n",
  ],
  [".env", "LP_STUDIO_E2E_IMPORT_SECRET=must-never-be-copied\n"],
]);

const writeImportFixture = async (root: string): Promise<void> => {
  await mkdir(join(root, "assets"), { recursive: true });
  await mkdir(join(root, "docs"), { recursive: true });
  for (const [path, content] of importFixture) {
    await writeFile(join(root, path), content, {
      encoding: "utf8",
      flag: "wx",
    });
  }
};

const importSourceSnapshot = async (
  root: string,
): Promise<{ sha256: string; byteLength: number }> => {
  const hash = createHash("sha256");
  let byteLength = 0;
  const paths = await readdir(root, { recursive: true });
  paths.sort();
  for (const path of paths) {
    const metadata = await lstat(join(root, path));
    const kind = metadata.isDirectory()
      ? "directory"
      : metadata.isFile()
        ? "file"
        : metadata.isSymbolicLink()
          ? "symlink"
          : "other";
    hash.update(`${kind}:${path}`, "utf8");
    hash.update(new Uint8Array([0]));
    if (metadata.isFile()) {
      const bytes = await readFile(join(root, path));
      hash.update(bytes);
      byteLength += bytes.byteLength;
    }
  }
  return { sha256: hash.digest("hex"), byteLength };
};

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
  const importRoot = await mkdtemp(
    join(tmpdir(), "synapsegit-lp-studio-import-e2e-"),
  );
  await writeImportFixture(importRoot);
  const sourceSnapshot = await importSourceSnapshot(importRoot);
  const server = spawn(resolve("target/debug/synapsegit-lp-local-server"), [], {
    cwd: process.cwd(),
    env: {
      ...process.env,
      LP_STUDIO_EDITOR_PORT: "0",
      LP_STUDIO_PREVIEW_PORT: "0",
      LP_STUDIO_STATE_ROOT: stateRoot,
      LP_STUDIO_WEB_DIST: resolve("apps/web/dist"),
      LP_STUDIO_IMPORT_ROOT: importRoot,
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
    process.env.LP_STUDIO_E2E_IMPORT_ROOT = importRoot;
    process.env.LP_STUDIO_E2E_IMPORT_SOURCE_SHA256 = sourceSnapshot.sha256;
    process.env.LP_STUDIO_E2E_IMPORT_SOURCE_BYTES = String(
      sourceSnapshot.byteLength,
    );
  } catch (error) {
    await stopServer(server);
    await rm(stateRoot, { recursive: true, force: true });
    await rm(importRoot, { recursive: true, force: true });
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
    await rm(importRoot, { recursive: true, force: true });
    delete process.env.LP_STUDIO_E2E_EDITOR_ORIGIN;
    delete process.env.LP_STUDIO_E2E_PREVIEW_ORIGIN;
    delete process.env.LP_STUDIO_E2E_IMPORT_ROOT;
    delete process.env.LP_STUDIO_E2E_IMPORT_SOURCE_SHA256;
    delete process.env.LP_STUDIO_E2E_IMPORT_SOURCE_BYTES;
  };
}
