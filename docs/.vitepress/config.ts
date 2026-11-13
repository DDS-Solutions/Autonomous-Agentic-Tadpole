/**
 * @docs ARCHITECTURE:Documentation
 * 
 * ### AI Assist Note
 * **Core technical resource for the Tadpole OS Sovereign infrastructure.**
 * Minimal VitePress documentation configuration.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Configuration error or missing route.
 * - **Telemetry Link**: Search `[config]` in observability traces.
 */

export default {
  title: 'Tadpole OS',
  description: 'Sovereign Agent Swarm Operating System',
  themeConfig: {
    nav: [
      { text: 'Home', link: '/' },
      { text: 'Architecture', link: '/ARCHITECTURE' },
      { text: 'API Reference', link: '/API_REFERENCE' },
      { text: 'Security', link: '/SECURITY' },
      { text: 'Operations', link: '/OPERATIONS_MANUAL' },
      { text: 'Capacity', link: '/CAPACITY_PLANNING' }
    ],
    sidebar: [
      {
        text: 'Core Platform',
        items: [
          { text: 'Overview', link: '/' },
          { text: 'Architecture', link: '/ARCHITECTURE' },
          { text: 'API Reference', link: '/API_REFERENCE' },
          { text: 'UI & Design Spec', link: '/design' }
        ]
      },
      {
        text: 'Security & Operations',
        items: [
          { text: 'Security Model', link: '/SECURITY' },
          { text: 'Operations Manual', link: '/OPERATIONS_MANUAL' },
          { text: 'Runbook Index', link: '/RUNBOOK_INDEX' },
          { text: 'Capacity Planning', link: '/CAPACITY_PLANNING' },
          { text: 'Data Retention', link: '/DATA_RETENTION' }
        ]
      }
    ]
  }
};

// Metadata: [config]
