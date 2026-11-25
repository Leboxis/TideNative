import { cn } from "@/lib/utils";

interface DolbyAudioLogoProps {
  size?: "sm" | "md";
  className?: string;
}

export function DolbyAudioLogo({ size = "sm", className }: DolbyAudioLogoProps) {
  return (
    <span
      role="img"
      aria-label="Dolby Audio"
      title="Dolby Audio"
      className={cn(
        "inline-flex shrink-0 items-center justify-center rounded-[4px] border border-white/25 bg-white/[0.10] text-white select-none",
        size === "md" ? "h-5 w-5" : "h-4 w-4",
        className,
      )}
    >
      <svg
        viewBox="0 0 24 16"
        fill="currentColor"
        aria-hidden="true"
        className={size === "md" ? "h-3 w-3.5" : "h-2.5 w-3"}
      >
        <path d="M1 1h3.2A7 7 0 0 1 4.2 15H1V1Zm22 0h-3.2a7 7 0 0 0 0 14H23V1Z" />
      </svg>
    </span>
  );
}
