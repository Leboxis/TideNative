import { Disc3 } from "lucide-react";
import { useEffect, useState } from "react";
import { type CoverSize, coverUrl } from "@/lib/covers";
import { cn } from "@/lib/utils";

interface CoverImageProps {
  coverId: string | null | undefined;
  alt: string;
  size?: CoverSize;
  className?: string;
  rounded?: "md" | "lg" | "xl" | "2xl" | "full";
}

const RADIUS_MAP = {
  md: "rounded-md",
  lg: "rounded-lg",
  xl: "rounded-xl",
  "2xl": "rounded-2xl",
  full: "rounded-full",
};

export function CoverImage({
  coverId,
  alt,
  size = 320,
  className,
  rounded = "lg",
}: CoverImageProps) {
  const [failed, setFailed] = useState(false);
  const src = coverUrl(coverId, size);

  useEffect(() => {
    setFailed(false);
  }, [src]);

  if (!src || failed) {
    return (
      <div
        className={cn(
          "flex items-center justify-center bg-[#14141b] border border-white/[0.06] text-white/20 shrink-0",
          RADIUS_MAP[rounded],
          className,
        )}
      >
        <Disc3 className="h-1/3 w-1/3" />
      </div>
    );
  }

  return (
    <img
      src={src}
      alt={alt}
      loading="lazy"
      decoding="async"
      onError={() => setFailed(true)}
      className={cn("object-cover bg-[#111116] shrink-0", RADIUS_MAP[rounded], className)}
    />
  );
}
