const CROCKFORD = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/** A ULID: 48 bits of Unix ms then 80 random bits, as 26 Crockford base32 characters. */
export const ulid = (now: number = Date.now()): string => {
  let time = "";
  for (let i = 0, t = now; i < 10; i++, t = Math.floor(t / 32)) time = CROCKFORD[t % 32] + time;
  return time + randomBase32(16);
};

/** `count` random Crockford base32 characters. */
export const randomBase32 = (count: number): string =>
  Array.from(crypto.getRandomValues(new Uint8Array(count)), (b) => CROCKFORD[b % 32]).join("");
