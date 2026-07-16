import { mkdir, readdir, readFile, writeFile } from "node:fs/promises";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const docsDirectory = join(root, "website", "docs");
const publicDirectories = [
  join(root, "apps", "landing", "public"),
  join(root, "website", "static"),
];
const docsBaseUrl = "https://docs.trusin.my.id";

async function findMarkdownFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = await Promise.all(entries.map(async (entry) => {
    const path = join(directory, entry.name);
    return entry.isDirectory()
      ? findMarkdownFiles(path)
      : entry.isFile() && entry.name.endsWith(".md")
        ? [path]
        : [];
  }));
  return files.flat().sort();
}

function parseDocument(filePath, source) {
  const withoutFrontmatter = source.replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n?/, "").trim();
  const frontmatter = source.match(/^---\r?\n([\s\S]*?)\r?\n---/);
  const frontmatterTitle = frontmatter?.[1].match(/^title:\s*["']?(.+?)["']?\s*$/m)?.[1];
  const headingTitle = withoutFrontmatter.match(/^#\s+(.+)$/m)?.[1];
  const description = frontmatter?.[1].match(/^description:\s*["']?(.+?)["']?\s*$/m)?.[1];
  const route = relative(docsDirectory, filePath)
    .replace(/\.md$/, "")
    .replace(/\\/g, "/");

  return {
    content: withoutFrontmatter,
    description: description || `trusin documentation: ${headingTitle || frontmatterTitle || route}.`,
    title: frontmatterTitle || headingTitle || route,
    url: `${docsBaseUrl}/docs/${route}`,
  };
}

const files = await findMarkdownFiles(docsDirectory);
const documents = await Promise.all(files.map(async (filePath) => (
  parseDocument(filePath, await readFile(filePath, "utf8"))
)));

const llmsIndex = [
  "# trusin",
  "> trusin is an open-source, self-hosted webhook relay for reliable delivery operations.",
  "",
  "trusin receives webhook events, stores their history in Postgres, queues delivery with Redis, routes events to targets, retries retryable failures, and provides a dashboard, CLI/TUI, and MCP server.",
  "",
  "## Primary links",
  "- [Website](https://trusin.my.id/): Product overview and webhook relay guide.",
  "- [Documentation](https://docs.trusin.my.id/docs/intro): Product and deployment documentation.",
  "- [Webhook relay guide](https://trusin.my.id/webhook-relay): What a webhook relay is and when to use one.",
  "- [GitHub repository](https://github.com/adityaputra11/terusin): Source code, releases, and issue tracker.",
  "- [Dashboard](https://app.trusin.my.id/): Hosted application.",
  "",
  "## Documentation",
  ...documents.map((document) => `- [${document.title}](${document.url}): ${document.description}`),
  "",
  "## AI and integration notes",
  "- Use API tokens for CLI and MCP access; never request or expose a dashboard password.",
  "- The bundled MCP server runs locally over stdio and uses TERUSIN_TOKEN.",
  "- Webhook delivery is at-least-once; target handlers should be idempotent.",
  "- For complete documentation content, read https://trusin.my.id/llms-full.txt.",
  "",
].join("\n");

const llmsFull = [
  "# trusin Documentation",
  "> Complete public documentation for trusin, an open-source self-hosted webhook relay.",
  "",
  "Canonical documentation: https://docs.trusin.my.id/docs/intro",
  "Product website: https://trusin.my.id/",
  "",
  ...documents.flatMap((document) => [
    `<!-- Source: ${document.url} -->`,
    document.content,
    "",
    "---",
    "",
  ]),
].join("\n");

await Promise.all(publicDirectories.map((directory) => mkdir(directory, { recursive: true })));
await Promise.all(publicDirectories.flatMap((directory) => [
  writeFile(join(directory, "llms.txt"), llmsIndex),
  writeFile(join(directory, "llms-full.txt"), llmsFull),
]));

console.log(`Generated llms.txt and llms-full.txt from ${documents.length} documentation pages.`);
