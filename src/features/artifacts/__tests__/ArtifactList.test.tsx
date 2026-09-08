import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { ArtifactList } from '../components/ArtifactList';
import type { ImportedArtifact } from '../types';

describe('ArtifactList Component', () => {
  const mockArtifacts: ImportedArtifact[] = [
    {
      artifact: {
        artifactId: 'art-vid-1',
        kind: 'video',
        uri: 'artifact://art-vid-1',
        sha256: 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855',
        sizeBytes: 15728640, // ~15 MB
        mimeType: 'video/mp4',
      },
      originalFilename: 'sunset_4k.mp4',
      addedAt: '2026-09-08T00:00:00Z',
      isDeduplicated: false,
      metadata: {
        artifactId: 'art-vid-1',
        probeStatus: 'ok',
        durationMs: 35000,
        width: 3840,
        height: 2160,
        videoCodec: 'h264',
        audioCodec: 'aac',
        frameRate: 60,
        sampleRate: 48000,
        channels: 2,
        probeError: null,
      },
    },
    {
      artifact: {
        artifactId: 'art-img-2',
        kind: 'image',
        uri: 'artifact://art-img-2',
        sha256: 'ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb',
        sizeBytes: 204800, // 200 KB
        mimeType: 'image/png',
      },
      originalFilename: 'thumbnail.png',
      addedAt: '2026-09-08T01:00:00Z',
      isDeduplicated: true,
      metadata: {
        artifactId: 'art-img-2',
        probeStatus: 'unavailable',
        durationMs: null,
        width: null,
        height: null,
        videoCodec: null,
        audioCodec: null,
        frameRate: null,
        sampleRate: null,
        channels: null,
        probeError: 'FFprobe unavailable',
      },
    },
  ];

  it('renders artifacts with metadata and deduplication badges', () => {
    render(
      <ArtifactList
        artifacts={mockArtifacts}
        isLoading={false}
        onImportClick={vi.fn()}
      />
    );

    // First artifact
    expect(screen.getByText('sunset_4k.mp4')).toBeInTheDocument();
    expect(screen.getByText('3840x2160')).toBeInTheDocument();
    expect(screen.getByText('00:35')).toBeInTheDocument();
    expect(screen.getByText('h264')).toBeInTheDocument();
    expect(screen.getByText('60 fps')).toBeInTheDocument();

    // Second artifact
    expect(screen.getByText('thumbnail.png')).toBeInTheDocument();
    expect(screen.getByText('Deduplicated')).toBeInTheDocument();
    expect(screen.getByText('Probe Unavailable')).toBeInTheDocument();
  });

  it('renders empty state when no artifacts are present', () => {
    render(
      <ArtifactList
        artifacts={[]}
        isLoading={false}
        onImportClick={vi.fn()}
      />
    );

    expect(screen.getByText(/no media assets in this project/i)).toBeInTheDocument();
  });
});
