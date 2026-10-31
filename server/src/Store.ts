// A user's library in their Durable Object's SQLite: decks, cards, reviews, a cached schedule per card, and the
// sync change log. Mirrors crates/core's store so the Worker and the clients answer the same questions the same way.
import {
  ApiError,
  type Card,
  type CardQuery,
  type CardUpdate,
  type CardView,
  type Change,
  type Deck,
  type DeckSummary,
  type DeckUpdate,
  type NewCard,
  type NewDeck,
  type NextReview,
  type Review,
  type Stats,
  type SyncRequest,
  type SyncResponse,
} from "./Model.ts";
import type { Scheduler } from "./srs.ts";
import { ulid } from "./ulid.ts";

type Value = string | number | null | ArrayBuffer;

/** The subset of workerd's `SqlStorage` the store uses. */
export interface Sql {
  exec<T extends Record<string, Value>>(
    query: string,
    ...bindings: Array<unknown>
  ): { toArray(): T[]; one(): T };
}

export interface Database {
  readonly sql: Sql;
  readonly transaction: <T>(f: () => T) => T;
}

const NEW_PER_DECK_PER_DAY = 20;
const DAY = 86_400_000;
const DEFAULT_LIMIT = 50;

// Each migration runs once, in order, tracked by `meta.schema_version`. Append; never edit a shipped one.
// `seq` on decks, cards and reviews is the change log: the seq of the change that wrote the row's current value.
const MIGRATIONS = [
  `CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value);
   CREATE TABLE decks (
     id TEXT PRIMARY KEY, name TEXT NOT NULL, parent_id TEXT,
     created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted INTEGER NOT NULL DEFAULT 0,
     seq INTEGER NOT NULL
   );
   CREATE INDEX decks_seq ON decks (seq);
   CREATE TABLE cards (
     id TEXT PRIMARY KEY, deck_id TEXT NOT NULL, content TEXT NOT NULL, tags TEXT NOT NULL DEFAULT '[]',
     created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted INTEGER NOT NULL DEFAULT 0,
     seq INTEGER NOT NULL
   );
   CREATE INDEX cards_deck ON cards (deck_id, created_at);
   CREATE INDEX cards_seq ON cards (seq);
   CREATE TABLE reviews (
     id TEXT PRIMARY KEY, card_id TEXT NOT NULL, rating INTEGER NOT NULL, reviewed_at INTEGER NOT NULL,
     duration_ms INTEGER, seq INTEGER NOT NULL,
     prior_state TEXT -- derived: the card's state when it was reviewed
   );
   CREATE INDEX reviews_card ON reviews (card_id, reviewed_at, id);
   CREATE INDEX reviews_time ON reviews (reviewed_at);
   CREATE INDEX reviews_seq ON reviews (seq);
   -- derived from reviews, see recompute
   CREATE TABLE schedules (
     card_id TEXT PRIMARY KEY, state TEXT NOT NULL, due INTEGER NOT NULL,
     stability REAL NOT NULL, difficulty REAL NOT NULL, reps INTEGER NOT NULL, lapses INTEGER NOT NULL,
     last_review INTEGER, first_review INTEGER
   );
   CREATE INDEX schedules_due ON schedules (state, due);
   INSERT INTO meta (key, value) VALUES ('seq', 0);`,
];

const CARD_VIEW = `SELECT c.id, c.deck_id, c.content, c.tags, c.created_at, c.updated_at, c.deleted,
   s.state, s.due, s.stability, s.difficulty, s.reps, s.lapses, s.last_review
   FROM cards c JOIN schedules s ON s.card_id = c.id`;

// New cards that may be shown today: per deck, the oldest `20 - introduced today` new cards. `:scope` (?2) is a
// JSON array of deck ids, or NULL for every deck. Bindings: ?1 day start, ?2 scope, ?3 cap.
const NEW_QUEUE = `WITH intro AS (
     SELECT c.deck_id, COUNT(*) AS n FROM cards c JOIN schedules s ON s.card_id = c.id
     WHERE c.deleted = 0 AND s.first_review >= ?1 GROUP BY c.deck_id
   ), ranked AS (
     SELECT c.id, c.deck_id, c.created_at,
       ROW_NUMBER() OVER (PARTITION BY c.deck_id ORDER BY c.created_at, c.id) AS rn
     FROM cards c JOIN schedules s ON s.card_id = c.id
     WHERE c.deleted = 0 AND s.state = 'new'
       AND (?2 IS NULL OR c.deck_id IN (SELECT value FROM json_each(?2)))
   )
   SELECT r.id, r.deck_id, r.created_at FROM ranked r LEFT JOIN intro i ON i.deck_id = r.deck_id
   WHERE r.rn <= ?3 - COALESCE(i.n, 0)`;

// Reviewed cards that are due. Bindings: ?1 now, ?2 scope.
const DUE = `SELECT c.id, c.deck_id, s.due FROM cards c JOIN schedules s ON s.card_id = c.id
   WHERE c.deleted = 0 AND s.state != 'new' AND s.due <= ?1
     AND (?2 IS NULL OR c.deck_id IN (SELECT value FROM json_each(?2)))`;

type Row = Record<string, Value>;

/** Splits Markdown into sides on lines that are exactly `---`. Blank lines around each side are dropped. */
export const sides = (content: string): string[] => {
  const out: string[][] = [[]];
  // Rust's `str::lines` drops one trailing newline; splitting keeps an extra empty line, which trimming removes.
  for (const line of content.split("\n")) {
    if (line.replace(/\r$/, "") === "---") out.push([]);
    else out[out.length - 1]!.push(line);
  }
  return out.map((lines) => {
    const blank = (l: string) => l.trim() === "";
    let start = 0;
    let end = lines.length;
    while (start < end && blank(lines[start]!)) start++;
    while (end > start && blank(lines[end - 1]!)) end--;
    return lines
      .slice(start, end)
      .map((l) => l.replace(/\r$/, ""))
      .join("\n");
  });
};

export const normalizeTag = (tag: string) => tag.trim().replace(/^#+/, "").trim().toLowerCase();

/** Lowercase, trimmed, `#` stripped, deduplicated and sorted; empty tags are dropped. */
export const normalizeTags = (tags: ReadonlyArray<string>): string[] =>
  [...new Set(tags.map(normalizeTag).filter((t) => t !== ""))].sort();

/** A local edit's `updated_at`: now, but always later than the version it replaces so it wins last-writer-wins. */
const bump = (previous: number, now: number) => Math.max(now, previous + 1);

const deckName = (name: string) => {
  const trimmed = name.trim();
  if (trimmed === "") throw ApiError.invalid("deck name can't be empty");
  return trimmed;
};

const utcDayStart = (now: number) => now - (((now % DAY) + DAY) % DAY);

const toDeck = (r: Row): Deck => ({
  id: r.id as string,
  name: r.name as string,
  parent_id: (r.parent_id as string | null) ?? null,
  created_at: r.created_at as number,
  updated_at: r.updated_at as number,
  deleted: r.deleted === 1,
});

const toCard = (r: Row): Card => ({
  id: r.id as string,
  deck_id: r.deck_id as string,
  content: r.content as string,
  tags: JSON.parse(r.tags as string) as string[],
  created_at: r.created_at as number,
  updated_at: r.updated_at as number,
  deleted: r.deleted === 1,
});

const toReview = (r: Row): Review => ({
  id: r.id as string,
  card_id: r.card_id as string,
  rating: r.rating as 1 | 2 | 3 | 4,
  reviewed_at: r.reviewed_at as number,
  duration_ms: (r.duration_ms as number | null) ?? null,
});

const toCardView = (r: Row): CardView => {
  const card = toCard(r);
  return {
    ...card,
    sides: sides(card.content),
    schedule: {
      state: r.state as CardView["schedule"]["state"],
      due: r.due as number,
      stability: r.stability as number,
      difficulty: r.difficulty as number,
      reps: r.reps as number,
      lapses: r.lapses as number,
      last_review: (r.last_review as number | null) ?? null,
    },
  };
};

const sameDeck = (a: Deck, b: Deck) =>
  a.name === b.name &&
  a.parent_id === b.parent_id &&
  a.created_at === b.created_at &&
  a.updated_at === b.updated_at &&
  a.deleted === b.deleted;

const sameCard = (a: Card, b: Card) =>
  a.deck_id === b.deck_id &&
  a.content === b.content &&
  JSON.stringify(a.tags) === JSON.stringify(b.tags) &&
  a.created_at === b.created_at &&
  a.updated_at === b.updated_at &&
  a.deleted === b.deleted;

export class Store {
  constructor(
    private readonly db: Database,
    private readonly scheduler: Scheduler,
    private readonly clock: () => number = Date.now,
  ) {
    db.transaction(() => {
      db.sql.exec("CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value)");
      const version = Number(this.meta("schema_version") ?? 0);
      MIGRATIONS.slice(version).forEach((m) => db.sql.exec(m));
      this.setMeta("schema_version", MIGRATIONS.length);
    });
  }

  private rows(query: string, ...bindings: Array<unknown>): Row[] {
    return this.db.sql.exec<Row>(query, ...bindings).toArray();
  }

  private meta(key: string): Value | undefined {
    return this.rows("SELECT value FROM meta WHERE key = ?", key)[0]?.value;
  }

  private setMeta(key: string, value: Value) {
    this.db.sql.exec("INSERT OR REPLACE INTO meta (key, value) VALUES (?, ?)", key, value);
  }

  private nextSeq(): number {
    return this.rows("UPDATE meta SET value = value + 1 WHERE key = 'seq' RETURNING value")[0]!.value as number;
  }

  // Accounts

  /** Stores the account's secret hash. Fails if the account already exists. */
  createAccount(secretHash: string) {
    if (this.meta("secret_hash") !== undefined) throw new ApiError(409, "conflict", "account already exists");
    this.setMeta("secret_hash", secretHash);
  }

  secretHash(): string | undefined {
    return this.meta("secret_hash") as string | undefined;
  }

  // Decks

  decks(): DeckSummary[] {
    const now = this.clock();
    const decks = this.rows(
      "SELECT * FROM decks WHERE deleted = 0 ORDER BY name COLLATE NOCASE, id",
    ).map(toDeck);
    const counts = this.deckCounts(now);
    const own = (id: string) => counts.get(id) ?? [0, 0, 0];
    const index = new Map(decks.map((d, i) => [d.id, i]));
    // Each deck's counts include its subdecks.
    const totals = decks.map((d) => [...own(d.id)]);
    decks.forEach((deck, i) => {
      const seen = new Set([i]);
      let parent = deck.parent_id;
      while (parent !== null) {
        const p = index.get(parent);
        if (p === undefined || seen.has(p)) break;
        seen.add(p);
        own(deck.id).forEach((n, k) => (totals[p]![k]! += n));
        parent = decks[p]!.parent_id;
      }
    });
    return decks.map((deck, i) => {
      const [cards, due, fresh] = totals[i]!;
      return { ...deck, cards: cards!, due: due!, new: fresh! };
    });
  }

  private deckCounts(now: number): Map<string, [number, number, number]> {
    const counts = new Map<string, [number, number, number]>();
    const queries: Array<[string, Array<unknown>]> = [
      ["SELECT deck_id, COUNT(*) AS n FROM cards WHERE deleted = 0 GROUP BY deck_id", []],
      [`SELECT deck_id, COUNT(*) AS n FROM (${DUE}) GROUP BY deck_id`, [now, null]],
      [
        `SELECT deck_id, COUNT(*) AS n FROM (${NEW_QUEUE}) GROUP BY deck_id`,
        [utcDayStart(now), null, NEW_PER_DECK_PER_DAY],
      ],
    ];
    queries.forEach(([sql, bindings], k) => {
      for (const row of this.rows(sql, ...bindings)) {
        const entry = counts.get(row.deck_id as string) ?? [0, 0, 0];
        entry[k] = row.n as number;
        counts.set(row.deck_id as string, entry);
      }
    });
    return counts;
  }

  private getDeck(id: string): Deck | undefined {
    const row = this.rows("SELECT * FROM decks WHERE id = ?", id)[0];
    return row && toDeck(row);
  }

  deck(id: string): Deck {
    const deck = this.getDeck(id);
    if (deck === undefined || deck.deleted) throw ApiError.notFound("deck", id);
    return deck;
  }

  /** The deck and its live descendants. */
  private subtree(id: string): string[] {
    return this.rows(
      `WITH RECURSIVE t(id) AS (SELECT ? UNION SELECT d.id FROM decks d JOIN t ON d.parent_id = t.id
       WHERE d.deleted = 0) SELECT id FROM t`,
      id,
    ).map((r) => r.id as string);
  }

  private scope(deckId: string | undefined): string | null {
    if (deckId === undefined) return null;
    this.deck(deckId);
    return JSON.stringify(this.subtree(deckId));
  }

  private putDeck(deck: Deck, seq: number) {
    this.db.sql.exec(
      `INSERT INTO decks (id, name, parent_id, created_at, updated_at, deleted, seq) VALUES (?, ?, ?, ?, ?, ?, ?)
       ON CONFLICT (id) DO UPDATE SET name = excluded.name, parent_id = excluded.parent_id,
       created_at = excluded.created_at, updated_at = excluded.updated_at, deleted = excluded.deleted,
       seq = excluded.seq`,
      deck.id,
      deck.name,
      deck.parent_id,
      deck.created_at,
      deck.updated_at,
      deck.deleted ? 1 : 0,
      seq,
    );
  }

  createDeck(input: NewDeck): Deck {
    const name = deckName(input.name);
    const parentId = input.parent_id ?? null;
    if (parentId !== null) this.deck(parentId);
    const now = this.clock();
    const deck: Deck = { id: ulid(now), name, parent_id: parentId, created_at: now, updated_at: now, deleted: false };
    this.db.transaction(() => this.putDeck(deck, this.nextSeq()));
    return deck;
  }

  updateDeck(id: string, update: DeckUpdate): Deck {
    const deck = { ...this.deck(id) };
    if (update.name !== undefined) deck.name = deckName(update.name);
    if (update.parent_id !== undefined) {
      if (update.parent_id !== null) {
        this.deck(update.parent_id);
        if (this.subtree(deck.id).includes(update.parent_id)) {
          throw ApiError.invalid("a deck can't be moved inside itself");
        }
      }
      deck.parent_id = update.parent_id;
    }
    deck.updated_at = bump(deck.updated_at, this.clock());
    this.db.transaction(() => this.putDeck(deck, this.nextSeq()));
    return deck;
  }

  /** Soft-deletes the deck, its subdecks and their cards. */
  deleteDeck(id: string) {
    this.deck(id);
    const ids = JSON.stringify(this.subtree(id));
    const now = this.clock();
    this.db.transaction(() => {
      // One seq per row keeps the change log's "one value per seq" shape.
      for (const [table, column] of [
        ["decks", "id"],
        ["cards", "deck_id"],
      ] as const) {
        const rows = this.rows(
          `SELECT id FROM ${table} WHERE deleted = 0 AND ${column} IN (SELECT value FROM json_each(?))`,
          ids,
        );
        for (const row of rows) {
          this.db.sql.exec(
            `UPDATE ${table} SET deleted = 1, updated_at = MAX(?, updated_at + 1), seq = ? WHERE id = ?`,
            now,
            this.nextSeq(),
            row.id,
          );
        }
      }
    });
  }

  // Cards

  cards(query: CardQuery): CardView[] {
    const scope = this.scope(query.deck_id);
    const q = query.q === undefined || query.q === "" ? null : query.q;
    const tag = query.tag === undefined ? null : normalizeTag(query.tag);
    const dueBefore = query.due === true ? this.clock() : null;
    return this.rows(
      `${CARD_VIEW} WHERE c.deleted = 0
       AND (?1 IS NULL OR c.deck_id IN (SELECT value FROM json_each(?1)))
       AND (?2 IS NULL OR instr(lower(c.content), lower(?2)) > 0)
       AND (?3 IS NULL OR EXISTS (SELECT 1 FROM json_each(c.tags) WHERE value = ?3))
       AND (?4 IS NULL OR s.due <= ?4)
       ORDER BY c.created_at, c.id LIMIT ?5 OFFSET ?6`,
      scope,
      q,
      tag,
      dueBefore,
      query.limit ?? DEFAULT_LIMIT,
      query.offset ?? 0,
    ).map(toCardView);
  }

  private cardView(id: string): CardView | undefined {
    const row = this.rows(`${CARD_VIEW} WHERE c.id = ?`, id)[0];
    return row && toCardView(row);
  }

  card(id: string): CardView {
    const card = this.cardView(id);
    if (card === undefined || card.deleted) throw ApiError.notFound("card", id);
    return card;
  }

  private putCard(card: Card, seq: number) {
    this.db.sql.exec(
      `INSERT INTO cards (id, deck_id, content, tags, created_at, updated_at, deleted, seq)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT (id) DO UPDATE SET deck_id = excluded.deck_id,
       content = excluded.content, tags = excluded.tags, created_at = excluded.created_at,
       updated_at = excluded.updated_at, deleted = excluded.deleted, seq = excluded.seq`,
      card.id,
      card.deck_id,
      card.content,
      JSON.stringify(card.tags),
      card.created_at,
      card.updated_at,
      card.deleted ? 1 : 0,
      seq,
    );
  }

  /** Creates all the cards or none. */
  createCards(input: ReadonlyArray<NewCard>): CardView[] {
    input.forEach((card) => this.deck(card.deck_id));
    const now = this.clock();
    const ids = this.db.transaction(() =>
      input.map((card, i) => {
        // A ms apart so a batch keeps its order in the new-card queue.
        const at = now + i;
        const created: Card = {
          id: ulid(at),
          deck_id: card.deck_id,
          content: card.content,
          tags: normalizeTags(card.tags ?? []),
          created_at: at,
          updated_at: at,
          deleted: false,
        };
        this.putCard(created, this.nextSeq());
        this.recompute(created.id);
        return created.id;
      }),
    );
    return ids.map((id) => this.card(id));
  }

  updateCard(id: string, update: CardUpdate): CardView {
    const view = this.card(id);
    const card: { -readonly [K in keyof Card]: Card[K] } = {
      id: view.id,
      deck_id: view.deck_id,
      content: update.content ?? view.content,
      tags: update.tags === undefined ? view.tags : normalizeTags(update.tags),
      created_at: view.created_at,
      updated_at: bump(view.updated_at, this.clock()),
      deleted: false,
    };
    if (update.deck_id !== undefined) {
      this.deck(update.deck_id);
      card.deck_id = update.deck_id;
    }
    this.db.transaction(() => this.putCard(card, this.nextSeq()));
    return this.card(id);
  }

  deleteCard(id: string) {
    const { sides: _, schedule: __, ...view } = this.card(id);
    const card: Card = { ...view, deleted: true, updated_at: bump(view.updated_at, this.clock()) };
    this.db.transaction(() => this.putCard(card, this.nextSeq()));
  }

  // Reviewing

  nextReview(deckId: string | undefined): NextReview {
    const now = this.clock();
    const scope = this.scope(deckId);
    const day = utcDayStart(now);
    const count = (sql: string, ...bindings: Array<unknown>) =>
      this.rows(`SELECT COUNT(*) AS n FROM (${sql})`, ...bindings)[0]!.n as number;
    const due = count(DUE, now, scope);
    const fresh = count(NEW_QUEUE, day, scope, NEW_PER_DECK_PER_DAY);
    const id =
      this.rows(`SELECT id FROM (${DUE}) ORDER BY due, id LIMIT 1`, now, scope)[0]?.id ??
      this.rows(`SELECT id FROM (${NEW_QUEUE}) ORDER BY created_at, id LIMIT 1`, day, scope, NEW_PER_DECK_PER_DAY)[0]
        ?.id;
    return { card: id === undefined ? null : this.card(id as string), remaining: { due, new: fresh } };
  }

  answer(cardId: string, rating: number, durationMs: number | null | undefined): CardView {
    if (![1, 2, 3, 4].includes(rating)) {
      throw ApiError.invalid("rating must be 1 (again), 2 (hard), 3 (good) or 4 (easy)");
    }
    if (durationMs != null && durationMs < 0) throw ApiError.invalid("duration_ms can't be negative");
    this.card(cardId);
    const now = this.clock();
    const review: Review = {
      id: ulid(now),
      card_id: cardId,
      rating: rating as Review["rating"],
      reviewed_at: now,
      duration_ms: durationMs ?? null,
    };
    this.db.transaction(() => {
      this.insertReview(review);
      this.recompute(cardId);
    });
    return this.card(cardId);
  }

  stats(deckId: string | undefined): Stats {
    const now = this.clock();
    const scope = this.scope(deckId);
    const day = utcDayStart(now);
    const inScope = "c.deleted = 0 AND (?1 IS NULL OR c.deck_id IN (SELECT value FROM json_each(?1)))";
    const cards = this.rows(`SELECT COUNT(*) AS n FROM cards c WHERE ${inScope}`, scope)[0]!.n as number;
    const due = this.rows(`SELECT COUNT(*) AS n FROM (${DUE})`, now, scope)[0]!.n as number;
    const fresh = this.rows(`SELECT COUNT(*) AS n FROM (${NEW_QUEUE})`, day, scope, NEW_PER_DECK_PER_DAY)[0]!
      .n as number;
    const row = this.rows(
      `SELECT COALESCE(SUM(r.reviewed_at >= ?2), 0) AS today,
         AVG(CASE WHEN r.prior_state = 'review' AND r.reviewed_at >= ?3 THEN r.rating > 1 END) AS retention
       FROM reviews r JOIN cards c ON c.id = r.card_id
       WHERE ${inScope} AND r.reviewed_at >= MIN(?2, ?3)`,
      scope,
      day,
      now - 30 * DAY,
    )[0]!;
    return {
      cards,
      due,
      new: fresh,
      reviews_today: row.today as number,
      retention_30d: (row.retention as number | null) ?? null,
    };
  }

  /** Inserts a review unless one with its id exists. Returns whether it was new. */
  private insertReview(review: Review): boolean {
    if (this.rows("SELECT 1 AS x FROM reviews WHERE id = ?", review.id).length > 0) return false;
    const seq = this.nextSeq();
    this.db.sql.exec(
      "INSERT INTO reviews (id, card_id, rating, reviewed_at, duration_ms, seq) VALUES (?, ?, ?, ?, ?, ?)",
      review.id,
      review.card_id,
      review.rating,
      review.reviewed_at,
      review.duration_ms,
      seq,
    );
    return true;
  }

  /**
   * Rebuilds a card's cached schedule by folding all its reviews in `(reviewed_at, id)` order, and records each
   * review's prior state (for retention stats). Does nothing for a card the store doesn't have yet.
   */
  private recompute(cardId: string) {
    const card = this.rows("SELECT created_at FROM cards WHERE id = ?", cardId)[0];
    if (card === undefined) return;
    const reviews = this.rows(
      "SELECT id, rating, reviewed_at, prior_state FROM reviews WHERE card_id = ? ORDER BY reviewed_at, id",
      cardId,
    ).filter((r) => [1, 2, 3, 4].includes(r.rating as number));
    const folds = reviews.map((_, i) =>
      i === 0
        ? null
        : this.scheduler.fold(
            card.created_at as number,
            reviews.slice(0, i).map((r) => ({ reviewed_at: r.reviewed_at as number, rating: r.rating as number })),
          ),
    );
    reviews.forEach((r, i) => {
      const prior = folds[i]?.state ?? "new";
      if (r.prior_state !== prior) this.db.sql.exec("UPDATE reviews SET prior_state = ? WHERE id = ?", prior, r.id);
    });
    const s = this.scheduler.fold(
      card.created_at as number,
      reviews.map((r) => ({ reviewed_at: r.reviewed_at as number, rating: r.rating as number })),
    );
    this.db.sql.exec(
      `INSERT OR REPLACE INTO schedules
       (card_id, state, due, stability, difficulty, reps, lapses, last_review, first_review)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      cardId,
      s.state,
      s.due,
      s.stability,
      s.difficulty,
      s.reps,
      s.lapses,
      s.last_review,
      (reviews[0]?.reviewed_at as number | undefined) ?? null,
    );
  }

  // Sync

  /**
   * Applies a device's changes and answers with everything it hasn't seen. Decks and cards merge last-writer-wins
   * on `updated_at` (ties keep the stored value), reviews by union on `id`. The response holds every stored change
   * with `seq > cursor` except values this request itself wrote, plus the stored value of any change it lost.
   */
  sync(request: SyncRequest): SyncResponse {
    return this.db.transaction(() => {
      const written = new Set<string>();
      const lost = new Set<string>();
      const touched = new Set<string>();
      for (const change of request.changes) {
        const key = `${change.kind}:${change.value.id}`;
        if (change.kind === "deck") {
          const stored = this.getDeck(change.value.id);
          if (stored === undefined || change.value.updated_at > stored.updated_at) {
            this.putDeck(change.value, this.nextSeq());
            written.add(key);
            lost.delete(key);
          } else if (sameDeck(stored, change.value)) written.add(key);
          else lost.add(key);
        } else if (change.kind === "card") {
          const value = { ...change.value, tags: normalizeTags(change.value.tags) };
          const row = this.rows("SELECT * FROM cards WHERE id = ?", value.id)[0];
          const stored = row && toCard(row);
          if (stored === undefined || value.updated_at > stored.updated_at) {
            this.putCard(value, this.nextSeq());
            touched.add(value.id);
            written.add(key);
            lost.delete(key);
          } else if (sameCard(stored, value)) written.add(key);
          else lost.add(key);
        } else {
          if (this.insertReview(change.value)) touched.add(change.value.card_id);
          written.add(key);
        }
      }
      touched.forEach((id) => this.recompute(id));

      const cursor = request.cursor ?? 0;
      const changes: Array<Change & { seq: number }> = [];
      const pick = (kind: Change["kind"], row: Row) => {
        const key = `${kind}:${row.id as string}`;
        if (written.has(key) && !lost.has(key)) return;
        const seq = row.seq as number;
        if (kind === "deck") changes.push({ kind, value: toDeck(row), seq });
        else if (kind === "card") changes.push({ kind, value: toCard(row), seq });
        else changes.push({ kind, value: toReview(row), seq });
      };
      const lostIds = (kind: string) =>
        JSON.stringify([...lost].filter((k) => k.startsWith(`${kind}:`)).map((k) => k.slice(kind.length + 1)));
      for (const [kind, table] of [
        ["deck", "decks"],
        ["card", "cards"],
        ["review", "reviews"],
      ] as const) {
        this.rows(
          `SELECT * FROM ${table} WHERE seq > ? OR id IN (SELECT value FROM json_each(?))`,
          cursor,
          lostIds(kind),
        ).forEach((row) => pick(kind, row));
      }
      changes.sort((a, b) => a.seq - b.seq);
      return {
        cursor: Number(this.meta("seq")),
        changes: changes.map(({ seq: _, ...change }) => change as Change),
      };
    });
  }
}
