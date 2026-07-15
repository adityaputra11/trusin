import type {SidebarsConfig} from '@docusaurus/plugin-content-docs';

const sidebars: SidebarsConfig = {
  tutorialSidebar: [
    'intro',
    {type: 'category', label: 'Guides', items: ['guides/local-development', 'guides/webhooks', 'guides/authentication', 'guides/cli-and-mcp']},
    {type: 'category', label: 'Concepts', items: ['concepts/architecture', 'concepts/reliability']},
    {type: 'category', label: 'Learn', items: ['learn/webhook-delivery', 'learn/webhook-retries', 'learn/self-hosted-infrastructure', 'learn/webhook-relay-vs-queues']},
    {type: 'category', label: 'Reference', items: ['reference/api', 'reference/configuration']},
    {type: 'category', label: 'Operations', items: ['operations/testing', 'operations/deployment', 'operations/troubleshooting']},
  ],
};
export default sidebars;
