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

const createStorageMock = (): Storage => {
  let store: Record<string, string> = {};

  return {
    get length() {
      return Object.keys(store).length;
    },
    clear: vi.fn(() => {
      store = {};
    }),
    getItem: vi.fn((key: string) => store[key] ?? null),
    key: vi.fn((index: number) => Object.keys(store)[index] ?? null),
    removeItem: vi.fn((key: string) => {
      delete store[key];
    }),
    setItem: vi.fn((key: string, value: string) => {
      store[key] = String(value);
    }),
  };
};

vi.stubGlobal('localStorage', createStorageMock());
vi.stubGlobal('sessionStorage', createStorageMock());

// Mock BroadcastChannel for inter-tab comms testing
class MockBroadcastChannel {
  name: string;
  onmessage: ((ev: MessageEvent) => any) | null = null;
  onmessageerror: ((ev: MessageEvent) => any) | null = null;

  constructor(name: string) {
    this.name = name;
  }

  postMessage(_message: unknown) {
    // In a real mock, we might want to trigger onmessage on other instances
    // but for unit tests, we usually just spy on postMessage
  }

  close() {}
  addEventListener() {}
  removeEventListener() {}
  dispatchEvent() { return true; }
}

vi.stubGlobal('BroadcastChannel', MockBroadcastChannel);

let uuidCounter = 0;

// Mock crypto for UUID generation
vi.stubGlobal('crypto', {
  randomUUID: () => {
    uuidCounter += 1;
    return `00000000-0000-4000-8000-${uuidCounter.toString().padStart(12, '0')}`;
  },
  getRandomValues: (array: Uint8Array) => {
    array.fill(1);
    return array;
  },
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
