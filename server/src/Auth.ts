// Interim token accounts (docs/contract.md, Accounts): `rc_<user_id>_<32 base32 chars>`.
import * as Effect from "effect/Effect";
import { randomBase32, ulid } from "./ulid.ts";

const TOKEN = /^Bearer rc_([0-9A-HJKMNP-TV-Z]{26})_([0-9A-HJKMNP-TV-Z]{32})$/;

export interface Token {
  readonly userId: string;
  readonly secret: string;
}

/** Parses an `Authorization` header. */
export const parseToken = (header: string | undefined): Token | undefined => {
  const match = header === undefined ? null : TOKEN.exec(header.trim());
  return match === null ? undefined : { userId: match[1]!, secret: match[2]! };
};

export const newAccount = () => {
  const userId = ulid();
  const secret = randomBase32(32);
  return { userId, secret, token: `rc_${userId}_${secret}` };
};

export const sha256Hex = (value: string) =>
  Effect.promise(() => crypto.subtle.digest("SHA-256", new TextEncoder().encode(value))).pipe(
    Effect.map((digest) => Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, "0")).join("")),
  );

/** Whether `secret` hashes to `hash`, compared in constant time. */
export const verifySecret = (secret: string, hash: string) =>
  sha256Hex(secret).pipe(
    Effect.map((actual) => {
      if (actual.length !== hash.length) return false;
      let diff = 0;
      for (let i = 0; i < actual.length; i++) diff |= actual.charCodeAt(i) ^ hash.charCodeAt(i);
      return diff === 0;
    }),
  );
