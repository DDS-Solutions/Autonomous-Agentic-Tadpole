-- Add stt_engine column to agents table for speech-to-text preference persistence.
ALTER TABLE agents ADD COLUMN stt_engine TEXT;
