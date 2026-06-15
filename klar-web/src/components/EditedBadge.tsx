"use client";

import { useState } from "react";

function formatFull(dateStr: string): string {
  return new Date(dateStr).toLocaleString(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  });
}

export default function EditedBadge({ editedAt }: { editedAt: string }) {
  const [visible, setVisible] = useState(false);
  return (
    <span
      className="relative inline-flex items-center"
      onMouseEnter={() => setVisible(true)}
      onMouseLeave={() => setVisible(false)}
    >
      <span className="cursor-default text-xs text-muted-foreground underline decoration-dotted">
        edited
      </span>
      {visible && (
        <span className="pointer-events-none absolute bottom-full left-1/2 z-20 mb-1.5 -translate-x-1/2 whitespace-nowrap rounded bg-foreground px-2 py-1 text-xs text-background shadow-md">
          {formatFull(editedAt)}
        </span>
      )}
    </span>
  );
}