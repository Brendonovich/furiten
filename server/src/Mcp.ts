// MCP tools (docs/contract.md, MCP). They mirror the HTTP API and run against the caller's own library.
import * as Schema from "effect/Schema";
import { Tool, Toolkit } from "effect/ai";
import { CardView, Deck, DeckSummary, NewCard, NextReview, Ok, Rating, Stats, Ulid } from "./Model.ts";

const CONTENT =
  "A card is Markdown. A line containing only `---` splits it into sides: the text above is the front (the " +
  "question), the text below is revealed as the answer, and further `---` lines add more sides revealed in order. " +
  "Content with no `---` is a one-sided note. Example: `What is the capital of France?\\n---\\nParis`.";

export const ToolError = Schema.Struct({ code: Schema.String, message: Schema.String });

const deckId = (description: string) => Schema.optionalKey(Ulid.annotate({ description }));

export const listDecks = Tool.make("list_decks", {
  description:
    "List the user's decks with their card counts: `cards` (total), `due` (reviewed cards due now) and `new` " +
    "(new cards available today). Counts include subdecks. Use a deck's `id` for the other tools.",
  success: Schema.Struct({ decks: Schema.Array(DeckSummary) }),
  failure: ToolError,
}).annotate(Tool.Readonly, true);

export const createDeck = Tool.make("create_deck", {
  description: "Create a deck, optionally nested inside another deck (`parent_id`).",
  parameters: Schema.Struct({
    name: Schema.String.annotate({ description: "The deck's name." }),
    parent_id: deckId("The id of the deck to nest this one inside; omit for a top-level deck."),
  }),
  success: Deck,
  failure: ToolError,
});

export const searchCards = Tool.make("search_cards", {
  description:
    "Search cards. Every filter is optional and they combine. Results are ordered by creation time. " +
    `Each result has \`content\`, its \`sides\` and its \`schedule\`. ${CONTENT}`,
  parameters: Schema.Struct({
    deck_id: deckId("Only cards in this deck and its subdecks."),
    q: Schema.optionalKey(Schema.String.annotate({ description: "Text the content contains (case-insensitive)." })),
    tag: Schema.optionalKey(Schema.String.annotate({ description: "Only cards with this tag." })),
    due: Schema.optionalKey(Schema.Boolean.annotate({ description: "Only cards due now (including new cards)." })),
    limit: Schema.optionalKey(
      Schema.Int.check(Schema.isGreaterThanOrEqualTo(0)).annotate({ description: "Maximum results, default 50." }),
    ),
    offset: Schema.optionalKey(
      Schema.Int.check(Schema.isGreaterThanOrEqualTo(0)).annotate({ description: "Results to skip, for paging." }),
    ),
  }),
  success: Schema.Struct({ cards: Schema.Array(CardView) }),
  failure: ToolError,
}).annotate(Tool.Readonly, true);

export const createCards = Tool.make("create_cards", {
  description:
    `Create one or more cards in one call; prefer batching many cards into a single call. ${CONTENT} ` +
    "Tags are lowercased and a leading `#` is dropped. All cards are created, or none if any is invalid.",
  parameters: Schema.Struct({
    cards: Schema.Array(
      Schema.Struct({
        deck_id: NewCard.fields.deck_id.annotate({ description: "The deck to add the card to." }),
        content: Schema.String.annotate({ description: `The card's Markdown. ${CONTENT}` }),
        tags: Schema.optionalKey(Schema.Array(Schema.String)),
      }),
    ),
  }),
  success: Schema.Struct({ cards: Schema.Array(CardView) }),
  failure: ToolError,
});

export const updateCard = Tool.make("update_card", {
  description: `Edit a card's content or tags, or move it to another deck. Omitted fields are unchanged. ${CONTENT}`,
  parameters: Schema.Struct({
    id: Ulid.annotate({ description: "The card's id." }),
    content: Schema.optionalKey(Schema.String.annotate({ description: `New Markdown content. ${CONTENT}` })),
    tags: Schema.optionalKey(Schema.Array(Schema.String).annotate({ description: "Replaces all the card's tags." })),
    deck_id: Schema.optionalKey(Ulid.annotate({ description: "Move the card to this deck." })),
  }),
  success: CardView,
  failure: ToolError,
});

export const deleteCard = Tool.make("delete_card", {
  description: "Delete a card. Its review history is kept but it no longer appears anywhere.",
  parameters: Schema.Struct({ id: Ulid.annotate({ description: "The card's id." }) }),
  success: Ok,
  failure: ToolError,
}).annotate(Tool.Destructive, true);

export const nextReview = Tool.make("next_review", {
  description:
    "Get the next card to review: due cards first (most overdue first), then new cards (at most 20 new cards per " +
    "deck per day). Returns `card: null` when nothing is left, and how many due and new cards remain. Show the " +
    "user the first side, let them answer, reveal the rest, then grade it with review_card.",
  parameters: Schema.Struct({ deck_id: deckId("Only review this deck and its subdecks.") }),
  success: NextReview,
  failure: ToolError,
}).annotate(Tool.Readonly, true);

export const reviewCard = Tool.make("review_card", {
  description:
    "Grade a review of a card now: 1 again (forgot), 2 hard, 3 good, 4 easy. Returns the card with its new " +
    "schedule (FSRS-6), including when it is next due.",
  parameters: Schema.Struct({
    id: Ulid.annotate({ description: "The card's id." }),
    rating: Rating,
    duration_ms: Schema.optionalKey(Schema.Int.annotate({ description: "How long the answer took, in ms." })),
  }),
  success: CardView,
  failure: ToolError,
});

export const stats = Tool.make("stats", {
  description:
    "Review statistics: card count, cards due now, new cards available today, reviews today, and 30-day " +
    "retention (share of review-state cards recalled, null without data).",
  parameters: Schema.Struct({ deck_id: deckId("Only this deck and its subdecks.") }),
  success: Stats,
  failure: ToolError,
}).annotate(Tool.Readonly, true);

export const toolkit = Toolkit.make(
  listDecks,
  createDeck,
  searchCards,
  createCards,
  updateCard,
  deleteCard,
  nextReview,
  reviewCard,
  stats,
);
