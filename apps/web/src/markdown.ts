import { Marked } from "marked";

// Cards render as GitHub-flavoured Markdown. Raw HTML is shown as text: cards can arrive from sync and imports.
const marked = new Marked({
  gfm: true,
  breaks: true,
  renderer: {
    html: ({ text }) => escape(text),
    link: ({ href, text }) => `<a href="${escape(href)}" target="_blank" rel="noopener noreferrer">${text}</a>`,
  },
});

function escape(s: string): string {
  return s.replace(/[&<>"']/g, (c) => `&${{ "&": "amp", "<": "lt", ">": "gt", '"': "quot", "'": "#39" }[c]};`);
}

export function renderMarkdown(text: string): string {
  return marked.parse(text, { async: false });
}
