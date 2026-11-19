import { useSearchParams } from "@solidjs/router";
import { For, Show, createSignal, untrack, useContext } from "solid-js";
import { ApiError, checkSession, sameOrigin, type Session } from "./api.ts";
import { Field } from "./Field.tsx";
import { Icon } from "./Icon.tsx";
import { AppContext, sessionFromLink } from "./state.ts";

const serverLabel = (server: string) => server || location.host;

/**
 * Signing in: the web app has no library of its own, so it uses your desktop's, over web access turned on in its
 * Settings.
 */
export function SignIn() {
  const app = useContext(AppContext);
  const [link, setLink] = createSignal("");
  const [token, setToken] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(untrack(() => app.state.notice));

  const go = async (session: Session | null) => {
    setBusy(true);
    setError(null);
    try {
      if (!session) throw new Error("Paste the link from your desktop’s Settings, or its address and token");
      await checkSession(session);
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

  const hours = Array.from({ length: 24 }, (_, h) => h);
  const hourLabel = (h: number) => `${String(h).padStart(2, "0")}:00${h === 4 ? " (default)" : ""}`;
  const rollover = () => app.state.settings?.rollover_hour ?? 4;

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
          <h2 class="section-title">Review</h2>
          <p class="muted">
            The day starts at the hour you pick: its due cards are all there from then, and reviews before it count as
            the day before, so late-night studying doesn’t wander into tomorrow. It’s this library’s setting, wherever
            it’s opened.
          </p>
          <label class="select" title="Next day starts at">
            <Icon name="clock" />
            <span>Next day starts at {hourLabel(rollover())}</span>
            <select
              aria-label="Next day starts at"
              value={rollover()}
              onChange={(e) => void app.setRollover(Number(e.currentTarget.value))}
            >
              <For each={hours}>{(h) => <option value={h}>{hourLabel(h)}</option>}</For>
            </select>
          </label>
        </section>
        <section>
          <h2 class="section-title">Sync</h2>
          <p>Connected to your desktop at {serverLabel(session().server)}</p>
          <p class="muted">
            This is your computer’s library, live: changes show up on both straight away. It works while the Furiten
            app is open there, on the same network or over Tailscale.
          </p>
          <button data-component="button-v2" data-variant="neutral" data-size="touch" onClick={() => void app.refresh()}>
            Refresh
          </button>
        </section>
        <section>
          <h2 class="section-title">Access</h2>
          <p class="muted">
            This token opens your desktop’s library. Turning web access off there disconnects every browser.
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
        </section>
        <section>
          <button data-component="button-v2" data-variant="ghost-danger" data-size="touch" class="wide" onClick={() => app.signOut()}>
            Disconnect
          </button>
        </section>
      </div>
    </div>
  );
}
