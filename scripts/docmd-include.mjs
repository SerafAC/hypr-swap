/**
 * `::include[]` for docmd — the mechanism behind FR-084.
 *
 * A question is answered in exactly one document. Where that document is a contract under
 * `specs/`, the site page includes it rather than restating it, so the two cannot diverge
 * (FR-084, FR-084a). docmd has no transclusion of its own, so this plugin adds one directive
 * and nothing else.
 *
 *   ::include[../../specs/002-overlay-visuals/contracts/config.md]
 *   ::include[../../specs/002-overlay-visuals/contracts/style-values.md#colours]
 *
 * The path is relative to the including page. A `#slug` selects one section by its
 * GitHub-style heading slug and takes that section's body, without its own heading, up to the
 * next heading of the same or higher level.
 *
 * Two rules make an included fragment read as part of its host page:
 *
 * 1. **Headings are demoted to fit.** Included headings are shifted so the shallowest of them
 *    sits one level below the page heading the directive appears under. A contract's `##
 *    Schema` included beneath a page's `## The visual settings` therefore renders as `###`,
 *    and the page keeps one heading hierarchy rather than two interleaved ones.
 * 2. **Relative links are rewritten to the repository.** A link inside a contract points at a
 *    sibling under `specs/`, which is not a page on this site. Such links are resolved against
 *    the included file and re-pointed at `repoBlobUrl`, so they still reach the document they
 *    name (FR-084a — `specs/` stays authoritative and reachable).
 *
 * A missing file or an unknown slug throws, so the build fails naming both the page and the
 * target rather than publishing a page with a hole in it.
 */

import fs from 'node:fs';
import path from 'node:path';

export const plugin = {
  name: 'include',
  version: '1.0.0',
  capabilities: ['markdown', 'init', 'build'],
};

/** One directive, alone on its line. */
const DIRECTIVE = /^::include\[([^\]\s]+)\][ \t]*$/;

/** Where a relative link inside an included file is re-pointed. Set from docmd.config.mjs. */
let repoBlobUrl = '';
/** Absolute path of the repository root, which `repoBlobUrl` paths are relative to. */
let repoRoot = process.cwd();

export function onConfigResolved(config) {
  // docmd keys plugin options by the string the config used to name the plugin, which for a
  // local plugin is its path. Match on the file name rather than repeating that path here.
  const entry = Object.entries(config.plugins ?? {}).find(([key]) =>
    key.endsWith('docmd-include.mjs'),
  );
  repoBlobUrl = (entry?.[1]?.repoBlobUrl ?? '').replace(/\/+$/, '');
  repoRoot = process.cwd();
}

/**
 * GitHub's heading slug: lowercase, drop everything but word characters, spaces and hyphens,
 * then spaces to hyphens. `## Values that are deliberately absent` →
 * `values-that-are-deliberately-absent`.
 */
function slug(text) {
  return text
    .trim()
    .toLowerCase()
    .replace(/[^\w\s-]/g, '')
    .replace(/\s+/g, '-');
}

/**
 * Split into lines tagged with whether each is a heading. A `#` inside a fenced code block is
 * a comment, not a heading — the contracts are full of TOML fences whose comments start with
 * `#`, so fences are tracked rather than assumed absent.
 */
function scan(markdown) {
  let fence = null;
  return markdown.split('\n').map((line) => {
    const fenceMark = line.match(/^\s*(```+|~~~+)/);
    if (fenceMark) {
      const wasOpen = fence !== null;
      if (fence === null) fence = fenceMark[1][0];
      else if (fenceMark[1][0] === fence) fence = null;
      return { line, level: 0, fenced: wasOpen || fence !== null };
    }
    if (fence !== null) return { line, level: 0, fenced: true };
    const heading = line.match(/^(#{1,6})\s+(.*)$/);
    return heading
      ? { line, level: heading[1].length, text: heading[2].replace(/\s+#+\s*$/, ''), fenced: false }
      : { line, level: 0, fenced: false };
  });
}

/** Everything after the file's frontmatter block. */
function body(markdown) {
  const frontmatter = markdown.match(/^---\r?\n[\s\S]*?\r?\n---[ \t]*\r?\n/);
  return frontmatter ? markdown.slice(frontmatter[0].length) : markdown;
}

/** The lines of one section: its body, without its own heading. Null when no heading matches. */
function section(lines, wanted) {
  const start = lines.findIndex((l) => l.level > 0 && slug(l.text) === wanted);
  if (start < 0) return null;
  const level = lines[start].level;
  let end = start + 1;
  while (end < lines.length && !(lines[end].level > 0 && lines[end].level <= level)) end += 1;
  return { lines: lines.slice(start + 1, end), level };
}

/** Shift every heading by `by` levels, capped at 6, and drop a trailing `---` rule. */
function demote(lines, by) {
  return lines.map(({ line, level }) =>
    level > 0 ? '#'.repeat(Math.min(6, level + by)) + line.slice(level) : line,
  );
}

/**
 * Re-point relative links so they still resolve. `from` is the included file; the result is a
 * `repoBlobUrl` link to the same document. Absolute URLs, anchors and mail links are untouched.
 */
function rewriteLinks(text, from) {
  if (!repoBlobUrl) return text;
  return text.replace(/\]\((\.[^)\s]*?)(#[^)\s]*)?\)/g, (whole, target, anchor) => {
    const resolved = path.relative(repoRoot, path.resolve(path.dirname(from), target));
    if (resolved.startsWith('..')) return whole;
    return `](${repoBlobUrl}/${resolved}${anchor ?? ''})`;
  });
}

/** Resolve one directive to the Markdown it stands for. */
function expand(spec, pageFile, depth) {
  const [relative, wanted] = spec.split('#');
  const target = path.resolve(path.dirname(pageFile), relative);

  let source;
  try {
    source = fs.readFileSync(target, 'utf8');
  } catch {
    throw new Error(
      `${path.relative(repoRoot, pageFile)}: ::include[${spec}] — no such file: ` +
        path.relative(repoRoot, target),
    );
  }

  const lines = scan(body(source));
  let chosen;
  let shallowest;

  if (wanted) {
    const found = section(lines, wanted);
    if (!found) {
      throw new Error(
        `${path.relative(repoRoot, pageFile)}: ::include[${spec}] — ` +
          `${path.relative(repoRoot, target)} has no heading with the slug "${wanted}"`,
      );
    }
    chosen = found.lines;
    shallowest = found.level + 1;
  } else {
    // A whole document opens with its own title, and the page including it has already given
    // that section a heading of its own. Two titles for one section is one too many, so a
    // leading level-1 heading is dropped and the prose beneath it kept.
    chosen = lines;
    const first = chosen.findIndex((l) => l.line.trim() !== '');
    if (first >= 0 && chosen[first].level === 1) chosen = chosen.slice(first + 1);
    const levels = chosen.filter((l) => l.level > 0).map((l) => l.level);
    shallowest = levels.length ? Math.min(...levels) : depth + 1;
  }

  const text = demote(chosen, depth + 1 - shallowest).join('\n').trim();
  return rewriteLinks(text, target);
}

/**
 * Expand every directive in one page. Called per page, before docmd parses the Markdown, so
 * everything downstream — the search index, the table of contents, `docmd validate` — sees the
 * included text as ordinary page content.
 */
export function onBeforeParse(src, _frontmatter, filePath) {
  if (!filePath || !src.includes('::include[')) return src;

  const lines = scan(src);
  let depth = 1;
  return lines
    .map(({ line, level, fenced }) => {
      if (level > 0) {
        depth = level;
        return line;
      }
      const directive = fenced ? null : line.match(DIRECTIVE);
      return directive ? expand(directive[1], filePath, depth) : line;
    })
    .join('\n');
}
