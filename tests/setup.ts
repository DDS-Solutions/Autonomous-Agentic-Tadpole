/**
 * @docs ARCHITECTURE:Quality:Verification
 * 
 * ### AI Assist Note
 * **Core technical resource for the Tadpole OS Sovereign infrastructure.**
 * Handles reactive state and high-fidelity user interactions.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: UI regression, hook desync, or API timeout.
 * - **Telemetry Link**: Search `[setup]` in observability traces.
 */

import '@testing-library/jest-dom';
import { vi } from 'vitest';

// Mock BroadcastChannel for inter-tab comms testing
class MockBroadcastChannel {
  name: string;
  onmessage: ((ev: MessageEvent) => any) | null = null;
  onmessageerror: ((ev: MessageEvent) => any) | null = null;

  constructor(name: string) {
    this.name = name;
  }

  postMessage(message: any) {
    // In a real mock, we might want to trigger onmessage on other instances
    // but for unit tests, we usually just spy on postMessage
  }

  close() {}
  addEventListener() {}
  removeEventListener() {}
  dispatchEvent() { return true; }
}

vi.stubGlobal('BroadcastChannel', MockBroadcastChannel);

// Mock crypto for UUID generation
vi.stubGlobal('crypto', {
  randomUUID: () => 'mock-uuid-' + Math.random().toString(36).substring(2, 9),
  subtle: {
    digest: () => Promise.resolve(new Uint8Array(32)),
  }
});

// Mock IntersectionObserver
class MockIntersectionObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}

vi.stubGlobal('IntersectionObserver', MockIntersectionObserver);

// Mock ResizeObserver
class MockResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}

vi.stubGlobal('ResizeObserver', MockResizeObserver);

// Metadata: [setup]
