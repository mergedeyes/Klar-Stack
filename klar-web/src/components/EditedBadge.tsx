"use client";

import { useState } from "react";
import { useFormatter, useTranslations } from "next-intl";

export default function EditedBadge({ editedAt }: { editedAt: string }) {
  const t = useTranslations("post");
  const format = useFormatter();
  const [visible, setVisible] = useState(false);
  return (
    <span
      className="relative inline-flex items-center"
      onMouseEnter={() => setVisible(true)}
      onMouseLeave={() => setVisible(false)}
    >
      <span className="cursor-default text-xs text-muted-foreground underline decoration-dotted">
        {t("edited")}
      </span>
      {visible && (
        <span className="pointer-events-none absolute bottom-full left-1/2 z-20 mb-1.5 -translate-x-1/2 whitespace-nowrap rounded bg-foreground px-2 py-1 text-xs text-background shadow-md">
          {format.dateTime(new Date(editedAt), { dateStyle: "medium", timeStyle: "short" })}
        </span>
      )}
    </span>
  );
}
