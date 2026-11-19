// What Furiten does, as Furiten cards: each a list of Markdown sides, the first one the prompt.

export const CARDS: string[][] = [
  [
    "How do I write a card?",
    "In **Markdown**, with a line of `---` between the sides. A card has as many sides as it needs.",
  ],
  [
    "When will I see it again?",
    "Just before you’d forget it. **FSRS-6** picks the day from how each review went, and all you answer is " +
      "*Remembered* or *Forgot*.",
  ],
  [
    "Does it work offline?",
    "Everywhere. Every device keeps the whole library, reviews and edits with no connection, and catches up when " +
      "it’s back. Reviews from two devices never conflict.",
  ],
  [
    "Where does my library sync?",
    "With a `furiten-hub` on your own NAS, Pi or VPS, or straight from your desktop to your phone. No ports to open.",
  ],
  [
    "Can my agent use it?",
    "Everything the app does is a command, with `--json`. There’s an HTTP API and an MCP server too.\n\n" +
      '```\nfuriten review --json\n```',
  ],
  [
    "Can I cram before a test?",
    "Review any deck, tag or search, due or not. Cramming leaves the schedule as it was.",
  ],
];
