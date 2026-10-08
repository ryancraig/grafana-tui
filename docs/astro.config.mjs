import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// Published at https://ryancraig.github.io/grafana-tui/ by
// .github/workflows/docs.yml. Links between pages use the /grafana-tui/ base.
export default defineConfig({
  site: 'https://ryancraig.github.io',
  base: '/grafana-tui',
  integrations: [
    starlight({
      title: 'grafana-tui',
      description: 'Grafana-like Prometheus dashboards in your terminal.',
      social: [
        {
          icon: 'github',
          label: 'GitHub',
          href: 'https://github.com/ryancraig/grafana-tui',
        },
      ],
      editLink: {
        baseUrl: 'https://github.com/ryancraig/grafana-tui/edit/main/docs/',
      },
      customCss: ['./src/styles/custom.css'],
      sidebar: [
        {
          label: 'Getting Started',
          items: [
            { label: 'Installation', slug: 'installation' },
            { label: 'Quick Start', slug: 'quick-start' },
            { label: 'Configuration', slug: 'configuration' },
          ],
        },
        {
          label: 'Guides',
          items: [
            { label: 'Grafana Dashboard Import', slug: 'grafana-dashboard-import' },
            { label: 'External Annotations', slug: 'annotations' },
            { label: 'Exporting and Recording', slug: 'exporting-and-recording' },
            { label: 'Keyboard and Mouse', slug: 'keyboard-and-mouse' },
            { label: 'Examples', slug: 'examples' },
          ],
        },
        {
          label: 'Reference',
          items: [
            { label: 'Grafana Compatibility', slug: 'grafana-compatibility' },
            { label: 'Troubleshooting', slug: 'troubleshooting' },
          ],
        },
      ],
    }),
  ],
});
