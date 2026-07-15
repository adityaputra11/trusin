import type {SidebarsConfig} from '@docusaurus/plugin-content-docs';

const sidebars: SidebarsConfig = {
  tutorialSidebar: [
    'intro',
    {type: 'category', label: 'Panduan', items: ['guides/local-development', 'guides/webhooks', 'guides/authentication', 'guides/cli-and-mcp']},
    {type: 'category', label: 'Konsep', items: ['concepts/architecture', 'concepts/reliability']},
    {type: 'category', label: 'Referensi', items: ['reference/api', 'reference/configuration']},
    {type: 'category', label: 'Operasional', items: ['operations/testing', 'operations/deployment', 'operations/troubleshooting']},
  ],
};
export default sidebars;
