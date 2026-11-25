import { Play } from "lucide-react";
import { CoverImage } from "@/components/media/CoverImage";
import type { CoverSize } from "@/lib/covers";
import { cn } from "@/lib/utils";

interface MediaCardProps {
  coverId: string | null | undefined;
  title: string;
  subtitle?: string | null;
  onOpen: () => void;
  onQuickPlay?: () => void;
  quickPlayLabel?: string;
  size?: CoverSize;
  rounded?: "xl" | "full";
  className?: string;
}

export function MediaCard({
  coverId,
  title,
  subtitle,
  onOpen,
  onQuickPlay,
  quickPlayLabel,
  size = 320,
  rounded = "xl",
  className,
}: MediaCardProps) {
  return (
    <div
      className={cn(
        "group relative card-oled flex flex-col rounded-2xl p-3 text-left transition-[transform,border-color] duration-150 hover:-translate-y-0.5 hover:border-white/20",
        className,
      )}
    >
      <button
        type="button"
        onClick={onOpen}
        className="flex w-full flex-col text-left cursor-pointer focus-visible:outline-none"
      >
        <div className="relative mb-3 aspect-square w-full overflow-hidden rounded-xl">
          <CoverImage
            coverId={coverId}
            alt={title}
            size={size}
            className="h-full w-full"
            rounded={rounded}
          />
        </div>
        <p className="truncate text-sm font-semibold text-white">{title}</p>
        {subtitle !== undefined && (
          <p className="mt-0.5 truncate text-xs text-white/60 tabular-nums">{subtitle ?? ""}</p>
        )}
      </button>

      {onQuickPlay && (
        <button
          type="button"
          aria-label={quickPlayLabel ?? `Lire ${title}`}
          onClick={(e) => {
            e.stopPropagation();
            onQuickPlay();
          }}
          className="absolute right-5 bottom-[58px] z-10 flex h-9 w-9 translate-y-1 items-center justify-center rounded-full bg-white text-[#050507] opacity-0 shadow-xl transition-[opacity,transform] duration-150 hover:scale-105 active:scale-95 group-hover:translate-y-0 group-hover:opacity-100 group-focus-within:translate-y-0 group-focus-within:opacity-100 focus-visible:translate-y-0 focus-visible:opacity-100 cursor-pointer"
        >
          <Play className="h-4 w-4 fill-current translate-x-[1px]" />
        </button>
      )}
    </div>
  );
}
