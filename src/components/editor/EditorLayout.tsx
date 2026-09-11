import React from 'react';

interface EditorLayoutProps {
  toolbar: React.ReactNode;
  assetBin: React.ReactNode;
  previewPlayer: React.ReactNode;
  timelineTracks: React.ReactNode;
}

export const EditorLayout: React.FC<EditorLayoutProps> = ({
  toolbar,
  assetBin,
  previewPlayer,
  timelineTracks,
}) => {
  return (
    <div className="flex flex-col h-[calc(100vh-4rem)] w-full bg-gray-950 text-gray-100 overflow-hidden font-sans select-none">
      {/* Top Toolbar */}
      <div className="shrink-0">{toolbar}</div>

      {/* Middle Workspace: Asset Bin + Preview Player */}
      <div className="flex flex-1 min-h-0 border-b border-gray-800">
        {assetBin}
        <div className="flex-1 min-w-0 flex flex-col">{previewPlayer}</div>
      </div>

      {/* Bottom Timeline Section */}
      <div className="h-64 shrink-0 flex flex-col bg-gray-950/80 border-t border-gray-800">
        <div className="flex-1 overflow-auto">{timelineTracks}</div>
      </div>
    </div>
  );
};
