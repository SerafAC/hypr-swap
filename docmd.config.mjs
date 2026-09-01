/**
 * The documentation site (FR-076 — FR-084b), built by docmd.
 *
 * The pages are the plain Markdown under `docs/`; docmd generates the site from that tree. This
 * file exists only for the four things the tree cannot say for itself: where the site is
 * published, in what order the pages read, what it is, and where `::include[]` should send a
 * link it rewrites.
 */
export default {
  title: 'hypr-swap',

  // GitHub Pages serves the site from a repository subpath. docmd derives the site's base path
  // from this URL, so `/hypr-swap/` is stated once, here, and nowhere else (FR-078). The dev
  // server ignores it and serves from `/`.
  url: 'https://serafac.github.io/hypr-swap/',

  src: 'docs',
  out: 'site',

  // Order is editorial, so it is stated rather than derived from filenames. The two sections are
  // FR-077's: an end-user guide and a developer guide, each complete on its own.
  navigation: [
    { title: 'Overview', path: '/', icon: 'home' },
    {
      title: 'User guide',
      icon: 'book-open',
      collapsible: true,
      children: [
        { title: 'Installing', path: '/user/install/' },
        { title: 'Binding the shortcuts', path: '/user/binds/' },
        { title: 'Configuration', path: '/user/configuration/' },
        { title: 'Appearance and themes', path: '/user/styling/' },
        { title: 'Program icons', path: '/user/icons/' },
        { title: 'Troubleshooting', path: '/user/troubleshooting/' },
      ],
    },
    {
      title: 'Developer guide',
      icon: 'code',
      collapsible: true,
      children: [
        { title: 'Architecture', path: '/dev/architecture/' },
        { title: 'Spec-driven workflow', path: '/dev/workflow/' },
        { title: 'Testing', path: '/dev/testing/' },
        { title: 'Verification', path: '/dev/verification/' },
        { title: 'Releasing', path: '/dev/releasing/' },
      ],
    },
  ],

  theme: {
    name: 'default',
    defaultMode: 'dark',
  },

  // Nothing on this site is authored as HTML — the pages are prose and the contracts they
  // include are tables and fences. Escaping raw HTML rather than passing it through means a
  // stray `<path>` in a contract renders as the text it is instead of silently disappearing
  // into the markup, which is the failure a reader would never notice.
  security: { html: 'escape' },

  editLink: {
    enabled: true,
    baseUrl: 'https://github.com/SerafAC/hypr-swap/edit/master',
  },

  plugins: {
    // docmd loads an AI-assistant client on every page by default, which carries a cloud relay
    // endpoint, and writes an "Open Knowledge Format" bundle for AI agents beside the site. This
    // project publishes neither. FR-071 states that the program makes no network access and sends
    // no telemetry, and the site that says so should not itself ship a 99 KB chat client pointed
    // at a third party — inert without a project id or not.
    ai: false,
    okf: false,

    seo: {
      defaultDescription:
        'An Alt-Tab-style workspace switcher with cross-monitor swapping for Hyprland.',
    },

    // `::include[]` — the mechanism behind FR-084. `repoBlobUrl` is where a relative link inside
    // an included contract is re-pointed: those documents stay authoritative and stay in the
    // repository rather than on this site (FR-084a), so a link to one must leave the site.
    './scripts/docmd-include.mjs': {
      repoBlobUrl: 'https://github.com/SerafAC/hypr-swap/blob/master',
    },
  },
};
