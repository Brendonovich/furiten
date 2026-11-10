import { useSearchParams } from "@solidjs/router";
import { createSignal, Show, untrack, useContext } from "solid-js";
import { ApiError, checkSession, createAccount, sameOrigin, type Session } from "./api.ts";
import { ago } from "./format.ts";
import { Field } from "./Field.tsx";
import { Icon } from "./Icon.tsx";
import { readSignIn } from "./routes.ts";
import { AppContext, isDesktop, sessionFromLink } from "./state.ts";

const serverLabel = (server: string) => server || location.host;

/**
 * Signing in. The web app has no library of its own: it uses your desktop's (with web access on in its Settings)
 * or a Furiten Cloud account.
 */
export function SignIn() {
  const app = useContext(AppContext);
  // Which tab is open is the URL's (`?via=cloud`), so a link can open either.
  const [search, setSearch] = useSearchParams();
  const mode = () => readSignIn(search);
  const setMode = (via: "desktop" | "cloud") => setSearch({ via: via === "cloud" ? via : undefined }, { replace: true });
  const [link, setLink] = createSignal("");
  const [server, setServer] = createSignal(sameOrigin);
  const [token, setToken] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(untrack(() => app.state.notice));

  const go = async (session: Session | null, create = false) => {
    setBusy(true);
    setError(null);
    try {
      if (create) session = await createAccount(server().trim().replace(/\/+$/, ""));
      if (!session) throw new Error("Paste the link from your desktop’s Settings, or its address and token");
      if (!create) await checkSession(session);
      setSearch({ via: undefined }, { replace: true });
      app.signIn(session);
    } catch (e) {
      const unauthorized = e instanceof ApiError && e.code === "unauthorized";
      setError(unauthorized ? "That token wasn’t accepted" : e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  /** A pasted link, or an address and a token typed separately. */
  const desktopSession = (): Session | null => {
    const fromLink = sessionFromLink(link());
    if (fromLink) return fromLink;
    const t = token().trim();
    if (!t) return null;
    const address = link().trim().replace(/\/+$/, "");
    const base = !address ? sameOrigin : /^https?:\/\//.test(address) ? address : `http://${address}`;
    return { server: base, token: t };
  };

  return (
    <div class="signin">
      <div class="signin-card">
        <img src="/icon.svg" alt="" width="64" height="64" />
        <h1>Furiten</h1>
        <div class="segmented text" role="radiogroup" aria-label="Where your decks are">
          <button
            role="radio"
            aria-checked={mode() === "desktop" ? "true" : "false"}
            class={{ active: mode() === "desktop" }}
            onClick={() => setMode("desktop")}
          >
            Your desktop
          </button>
          <button
            role="radio"
            aria-checked={mode() === "cloud" ? "true" : "false"}
            class={{ active: mode() === "cloud" }}
            onClick={() => setMode("cloud")}
          >
            Furiten Cloud
          </button>
        </div>
        <Show
          when={mode() === "desktop"}
          fallback={
            <>
              <p class="muted">Sign in with your account token, or start a new account.</p>
              <form
                onSubmit={(e) => {
                  e.preventDefault();
                  void go({ server: server().trim().replace(/\/+$/, ""), token: token().trim() });
                }}
              >
                <Field
                  placeholder={`Server (${location.host})`}
                  inputmode="url"
                  autocapitalize="none"
                  value={server()}
                  onInput={(e) => setServer(e.currentTarget.value)}
                />
                <Field
                  mono
                  placeholder="rc_…"
                  autocapitalize="none"
                  autocomplete="off"
                  value={token()}
                  onInput={(e) => setToken(e.currentTarget.value)}
                />
                <button data-component="button-v2" data-variant="contrast" data-size="touch" class="wide" type="submit" disabled={busy() || !token().trim()}>
                  Sign in
                </button>
              </form>
              <button data-component="button-v2" data-variant="neutral" data-size="touch" class="wide" disabled={busy()} onClick={() => void go(null, true)}>
                Create a new account
              </button>
            </>
          }
        >
          <p class="muted">
            On your computer, open Furiten’s Settings and turn on <b>Use this library from your phone</b>. Then scan
            its code, or paste its link here.
          </p>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void go(desktopSession());
            }}
          >
            <Field
              placeholder="http://your-mac:8787/#token=…"
              inputmode="url"
              autocapitalize="none"
              autocomplete="off"
              value={link()}
              onInput={(e) => setLink(e.currentTarget.value)}
            />
            <Show when={!sessionFromLink(link())}>
              <Field
                mono
                placeholder="Token (rw_…), if you typed the address"
                autocapitalize="none"
                autocomplete="off"
                value={token()}
                onInput={(e) => setToken(e.currentTarget.value)}
              />
            </Show>
            <button data-component="button-v2" data-variant="contrast" data-size="touch" class="wide" type="submit" disabled={busy() || !desktopSession()}>
              Connect
            </button>
          </form>
        </Show>
        <Show when={error()}>{(e) => <p class="error">{e()}</p>}</Show>
      </div>
    </div>
  );
}

export function Settings() {
  const app = useContext(AppContext);
  const [showToken, setShowToken] = createSignal(false);
  const [copied, setCopied] = createSignal(false);
  const session = () => app.state.session!;
  const server = () => session().server || location.origin;
  const command = () => `furiten sync login --server ${server()} --token ${session().token}`;

  return (
    <div class="screen">
      <header class="toolbar">
        <button class="icon-button" aria-label="Decks" onClick={() => app.setState((s) => void (s.drawer = true))}>
          <Icon name="menu" />
        </button>
        <h1 class="toolbar-title">Settings</h1>
        <span class="icon-button" />
      </header>
      <div class="settings">
        <section>
          <h2 class="section-title">Sync</h2>
          <Show
            when={isDesktop(session())}
            fallback={
              <>
                <p>Signed in to Furiten Cloud at {serverLabel(session().server)}</p>
                <p class="muted">
                  {app.state.lastSync ? `Last synced ${ago(app.state.lastSync)}` : "Not synced yet"}. Changes save
                  straight to the server, so your other devices get them on their next sync.
                </p>
              </>
            }
          >
            <p>Connected to your desktop at {serverLabel(session().server)}</p>
            <p class="muted">
              This is your computer’s library, live: changes show up on both straight away. It works while the Furiten
              app is open there, on the same network or over Tailscale.
            </p>
          </Show>
          <button data-component="button-v2" data-variant="neutral" data-size="touch" onClick={() => void app.refresh()}>
            Sync now
          </button>
        </section>
        <section>
          <h2 class="section-title">{isDesktop(session()) ? "Access" : "Your account"}</h2>
          <p class="muted">
            {isDesktop(session())
              ? "This token opens your desktop’s library. Turning web access off there disconnects every browser."
              : "Your token signs you in on other devices. Keep it secret."}
          </p>
          <div class="token mono" data-scroll-x>
            {showToken() ? session().token : session().token.slice(0, 3) + "•".repeat(24)}
          </div>
          <div class="row">
            <button data-component="button-v2" data-variant="neutral" data-size="touch" onClick={() => setShowToken(!showToken())}>
              <Icon name="eye" /> {showToken() ? "Hide" : "Show"}
            </button>
            <button
              data-component="button-v2" data-variant="neutral" data-size="touch"
              onClick={async () => {
                await navigator.clipboard?.writeText(session().token);
                setCopied(true);
                setTimeout(() => setCopied(false), 1500);
              }}
            >
              {copied() ? "Copied" : "Copy token"}
            </button>
          </div>
          <Show when={!isDesktop(session())}>
            <p class="muted small">On a desktop, sync this account with:</p>
            <pre class="command" data-scroll-x>
              {showToken() ? command() : command().replace(session().token, "<token>")}
            </pre>
          </Show>
        </section>
        <section>
          <button data-component="button-v2" data-variant="ghost-danger" data-size="touch" class="wide" onClick={() => app.signOut()}>
            {isDesktop(session()) ? "Disconnect" : "Sign out"}
          </button>
        </section>
      </div>
    </div>
  );
}
