// Runs the tests against the real Worker and Durable Object: `alchemy dev` on the `test` stage serves them in a
// local workerd, offline. Set FURITEN_BASE_URL to run the same black-box tests against a server that's already up.

let server: Promise<string> | undefined;
let child: ReturnType<typeof Bun.spawn> | undefined;

const start = async (): Promise<string> => {
  if (process.env.FURITEN_BASE_URL) return process.env.FURITEN_BASE_URL.replace(/\/$/, "");
  const cwd = new URL("..", import.meta.url).pathname;
  child = Bun.spawn(["bun", "node_modules/alchemy/bin/cli.js", "dev", "--stage", "test", "--no-input"], {
    cwd,
    stdout: "pipe",
    stderr: "pipe",
    env: { ...process.env, FURITEN_STATE: "local" },
  });
  let output = "";
  const decoder = new TextDecoder();
  const reader = (child.stdout as ReadableStream<Uint8Array>).getReader();
  const deadline = Date.now() + 120_000;
  while (Date.now() < deadline) {
    const { value, done } = await reader.read();
    if (done) break;
    output += decoder.decode(value);
    const url = /\[Api\] Started in \d+ms → (http:\/\/[^\s/]+)/.exec(output)?.[1];
    if (url) {
      // Keep draining so the child never blocks on a full pipe.
      void (async () => {
        while (!(await reader.read()).done);
      })();
      return url;
    }
  }
  child.kill();
  throw new Error(`alchemy dev didn't start:\n${output}\n${await new Response(child.stderr as ReadableStream).text()}`);
};

export const baseUrl = () => (server ??= start());

/** Stops the dev server and the workerd it runs (test/setup.ts calls this once, after every test file). */
export const stop = async () => {
  if (child === undefined) return;
  child.kill("SIGINT");
  const exited = await Promise.race([child.exited.then(() => true), Bun.sleep(10_000).then(() => false)]);
  if (!exited) child.kill("SIGKILL");
};

export interface Response<T = any> {
  readonly status: number;
  readonly body: T;
  readonly headers: Headers;
}

export const request = async <T = any>(
  method: string,
  path: string,
  options: { token?: string; body?: unknown; headers?: Record<string, string> } = {},
): Promise<Response<T>> => {
  const res = await fetch(`${await baseUrl()}${path}`, {
    method,
    headers: {
      ...(options.body === undefined ? {} : { "content-type": "application/json" }),
      ...(options.token === undefined ? {} : { authorization: `Bearer ${options.token}` }),
      ...options.headers,
    },
    body: options.body === undefined ? undefined : JSON.stringify(options.body),
  });
  const text = await res.text();
  try {
    return { status: res.status, body: text === "" ? undefined : JSON.parse(text), headers: res.headers };
  } catch {
    throw new Error(`${method} ${path}: ${res.status} ${text}`);
  }
};

/** A fresh account's API client. */
export const account = async () => {
  const created = await request<{ user_id: string; token: string }>("POST", "/v1/accounts");
  if (created.status !== 200) throw new Error(`POST /v1/accounts: ${created.status}`);
  const { token, user_id } = created.body;
  const call = <T = any>(method: string, path: string, body?: unknown) => request<T>(method, path, { token, body });
  return {
    token,
    userId: user_id,
    get: <T = any>(path: string) => call<T>("GET", path),
    post: <T = any>(path: string, body?: unknown) => call<T>("POST", path, body ?? {}),
    patch: <T = any>(path: string, body: unknown) => call<T>("PATCH", path, body),
    delete: <T = any>(path: string) => call<T>("DELETE", path),
  };
};

export type Account = Awaited<ReturnType<typeof account>>;

const CROCKFORD = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/** A ULID for entities a simulated device creates. */
export const ulid = (ms: number = Date.now()) => {
  let time = "";
  for (let i = 0, t = ms; i < 10; i++, t = Math.floor(t / 32)) time = CROCKFORD[t % 32] + time;
  return time + Array.from(crypto.getRandomValues(new Uint8Array(16)), (b) => CROCKFORD[b % 32]).join("");
};
