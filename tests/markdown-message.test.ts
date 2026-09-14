import assert from "node:assert/strict";
import test from "node:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";

// Same renderer configuration as src/components/dispatch/MarkdownMessage.tsx.
const render = (text: string) =>
  renderToStaticMarkup(createElement(Markdown, { remarkPlugins: [remarkGfm] }, text));

test("agent replies render nested lists, bold labels, and inline code", () => {
  const html = render(
    "- **Contributor guide:**\n  - dev setup\n  - PR checklist\n- **Privacy:** see `README.md`",
  );
  assert.match(html, /<strong>Contributor guide:<\/strong>/);
  assert.match(html, /<ul>[\s\S]*<ul>[\s\S]*<li>dev setup<\/li>/);
  assert.match(html, /<code>README.md<\/code>/);
  assert.doesNotMatch(html, /\*\*/);
});

test("GFM tables render as tables", () => {
  const html = render("| Decision | Why |\n| --- | --- |\n| SQLite | local-first |");
  assert.match(html, /<table>[\s\S]*<th>Decision<\/th>[\s\S]*<td>SQLite<\/td>/);
});

test("raw HTML in agent output is not injected", () => {
  const html = render('Hi <img src=x onerror="alert(1)"> there');
  assert.doesNotMatch(html, /<img/);
});
