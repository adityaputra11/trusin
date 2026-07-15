import type {Config} from '@docusaurus/types';
import type {Options, ThemeConfig} from '@docusaurus/preset-classic';
import {themes as prismThemes} from 'prism-react-renderer';

const config: Config = {
  title: 'Terusin',
  tagline: 'Self-hosted webhook relay yang cepat dan andal',
  favicon: 'img/terusin-mark.svg',
  url: 'https://terusin-dev.my.id',
  baseUrl: '/',
  organizationName: 'adityaputra11',
  projectName: 'terusin',
  onBrokenLinks: 'throw',
  onBrokenMarkdownLinks: 'warn',
  i18n: {defaultLocale: 'id', locales: ['id']},
  presets: [
    ['classic', {
      docs: {sidebarPath: './sidebars.ts', routeBasePath: 'docs'},
      blog: false,
      theme: {customCss: './src/css/custom.css'},
    } satisfies Options],
  ],
  themeConfig: {
    navbar: {
      title: '',
      logo: {alt: 'Terusin', src: 'img/terusin-logo.svg'},
      items: [
        {type: 'docSidebar', sidebarId: 'tutorialSidebar', position: 'left', label: 'Dokumentasi'},
        {href: 'https://github.com/adityaputra11/terusin', label: 'GitHub', position: 'right'},
      ],
    },
    footer: {
      style: 'dark',
      links: [{title: 'Docs', items: [
        {label: 'Mulai', to: '/docs/intro'},
        {label: 'API', to: '/docs/reference/api'},
        {label: 'Troubleshooting', to: '/docs/operations/troubleshooting'},
      ]}],
      copyright: `Copyright © ${new Date().getFullYear()} Terusin. Apache-2.0.`,
    },
    prism: {theme: prismThemes.github, darkTheme: prismThemes.dracula},
    colorMode: {defaultMode: 'dark', respectPrefersColorScheme: true},
  } satisfies ThemeConfig,
};

export default config;
