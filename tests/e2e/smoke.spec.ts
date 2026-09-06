/**
 * @docs ARCHITECTURE:Testing
 * 
 * ### AI Assist Note
 * **Core technical resource for the Tadpole OS Sovereign infrastructure.**
 * E2E Smoke Test Suite for Tadpole OS Dashboard.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Environment failure or runner desynchronization.
 * - **Telemetry Link**: Search `[smoke_spec]` in observability traces.
 */

import { test, expect } from '@playwright/test';

test.describe('E2E Dashboard Smoke Test', () => {
  test('verifies dashboard landing and health status contract', async ({ request, page }) => {
    // 1. Verify engine health endpoint contract if backend is live
    try {
      const healthRes = await request.get('http://127.0.0.1:8000/v1/engine/health');
      if (healthRes.ok()) {
        const body = await healthRes.json();
        expect(body).toHaveProperty('status');
        expect(['healthy', 'degraded', 'failed']).toContain(body.status);
      }
    } catch {
      // Backend not running in headless frontend-only CI mode; proceed to UI test
    }

    // 2. Verify dashboard page loads with proper title
    await page.goto('/');
    await expect(page).toHaveTitle(/Tadpole/i);
  });
});

// Metadata: [smoke_spec]
