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
      { text: 'Architecture', link: '/ARCHITECTURE' },
      { text: 'Operations', link: '/OPERATIONS_MANUAL' },
      { text: 'Capacity', link: '/CAPACITY_PLANNING' }
    ]
  }
};

// Metadata: [config]
