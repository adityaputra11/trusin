import type {Config} from '@docusaurus/types';
import type {Options, ThemeConfig} from '@docusaurus/preset-classic';
import {themes as prismThemes} from 'prism-react-renderer';

const config: Config = {
  title: 'trusin Documentation',
  tagline: 'Documentation for reliable, self-hosted webhook delivery',
  favicon: 'img/favicon.png',
  url: process.env.DOCS_URL || 'https://docs.trusin.my.id',
  baseUrl: '/',
  organizationName: 'adityaputra11',
  projectName: 'terusin',
  onBrokenLinks: 'throw',
  onBrokenMarkdownLinks: 'warn',
  i18n: {defaultLocale: 'en', locales: ['en']},
  presets: [
    ['classic', {
      docs: {sidebarPath: './sidebars.ts', routeBasePath: 'docs'},
      blog: false,
      theme: {customCss: './src/css/custom.css'},
    } satisfies Options],
  ],
  themeConfig: {
    navbar: {
      title: 'trusin',
      logo: {alt: 'trusin', src: 'img/favicon.png'},
      items: [
        {type: 'docSidebar', sidebarId: 'tutorialSidebar', position: 'left', label: 'Documentation'},
        {href: process.env.LANDING_URL || 'https://trusin.my.id', label: 'trusin', position: 'right'},
        {href: process.env.APP_URL || 'https://app.trusin.my.id', label: 'Open app', position: 'right'},
        {href: 'https://github.com/adityaputra11/terusin', label: 'GitHub', position: 'right'},
      ],
    },
    footer: {
      style: 'dark',
      links: [{title: 'Docs', items: [
        {label: 'Get started', to: '/docs/intro'},
        {label: 'API', to: '/docs/reference/api'},
        {label: 'Troubleshooting', to: '/docs/operations/troubleshooting'},
      ]}],
      copyright: `Copyright © ${new Date().getFullYear()} trusin. Apache-2.0.`,
    },
    prism: {theme: prismThemes.github, darkTheme: prismThemes.dracula},
    colorMode: {defaultMode: 'dark', respectPrefersColorScheme: true},
  } satisfies ThemeConfig,
};

export default config;
