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
import { APP_ROUTES } from '../../src/constants/routes';

test.describe('E2E Dashboard Smoke Test', () => {
  test('verifies dashboard landing and health status contract', async ({ request, page }) => {
    // 1. Verify engine health endpoint contract if backend is live
    const healthRes = await request.get('http://127.0.0.1:8000/v1/engine/health', { timeout: 3000 }).catch(() => null);
    if (healthRes?.ok()) {
      const body = await healthRes.json();
      expect(body).toHaveProperty('status');
      expect(['healthy', 'degraded', 'failed']).toContain(body.status);
    }

    // 2. Verify dashboard page loads with proper title
    await page.goto('/', { waitUntil: 'domcontentloaded' });
    await expect(page).toHaveTitle(/Tadpole/i);
    await expect(page.getByRole('main')).toBeVisible({ timeout: 30000 });
  });
});

test.describe('Registered page smoke checks', () => {
  for (const { path, label } of APP_ROUTES) {
    test(`renders ${label} (${path})`, async ({ page }) => {
      const errors: string[] = [];
      page.on('pageerror', error => errors.push(error.message));
      await page.goto(path, { waitUntil: 'domcontentloaded' });
      await expect(page.getByRole('main')).toBeVisible();
      await expect(page.getByText(/^Initializing Sector\.\.\.$/)).toHaveCount(0);
      await expect(page.getByText(/^Neural Fault$/i)).toHaveCount(0);
      await expect(page).toHaveTitle(/Tadpole/i);
      expect(new URL(page.url()).pathname).toBe(path);
      expect(errors).toEqual([]);
    });
  }
});

// Metadata: [smoke_spec]
