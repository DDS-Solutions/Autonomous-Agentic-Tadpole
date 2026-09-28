import { describe, it, expect } from 'vitest';
import { is_allowed_origin } from './base-api-service';

describe('is_allowed_origin Security Allowlist', () => {
    it('O-01: allows loopback http localhost', () => {
        expect(is_allowed_origin('http://localhost:8000')).toBe(true);
        expect(is_allowed_origin('http://localhost')).toBe(true);
    });

    it('O-02: rejects external https domains', () => {
        expect(is_allowed_origin('https://evil.example')).toBe(false);
        expect(is_allowed_origin('https://api.openai.com')).toBe(false);
    });

    it('O-03: rejects external http cleartext domains', () => {
        expect(is_allowed_origin('http://evil.example')).toBe(false);
    });

    it('O-04: rejects LAN and non-loopback addresses', () => {
        expect(is_allowed_origin('http://10.0.0.5:8000')).toBe(false);
        expect(is_allowed_origin('http://192.168.1.100:8000')).toBe(false);
    });

    it('O-05: allows official desktop tauri origin', () => {
        expect(is_allowed_origin('tauri://localhost')).toBe(true);
    });

    it('O-06: rejects file protocol', () => {
        expect(is_allowed_origin('file:///etc/passwd')).toBe(false);
    });

    it('O-07: rejects invalid and malformed URLs', () => {
        expect(is_allowed_origin('not-a-url')).toBe(false);
        expect(is_allowed_origin('')).toBe(false);
    });

    it('O-08: rejects subdomain and prefix bypass attempts', () => {
        expect(is_allowed_origin('https://localhost.evil.example')).toBe(false);
        expect(is_allowed_origin('http://localhost.evil.com:8000')).toBe(false);
    });

    it('O-09: allows 127.0.0.1 loopback', () => {
        expect(is_allowed_origin('http://127.0.0.1:8000')).toBe(true);
    });

    it('O-10: rejects 0.0.0.0 bind destination', () => {
        expect(is_allowed_origin('http://0.0.0.0:8000')).toBe(false);
    });
});
