import { cn } from "@/lib/utils";

export function Skeleton({ className }: { className?: string }) {
  return <div aria-hidden="true" className={cn("animate-pulse rounded-lg bg-white/[0.07]", className)} />;
}

export function DetailHeaderSkeleton({ rounded = "2xl" }: { rounded?: "2xl" | "full" }) {
  return (
    <div
      role="status"
      aria-label="Chargement en cours"
      className="flex flex-col gap-6 md:flex-row md:items-end"
    >
      <Skeleton
        className={cn("h-44 w-44 shrink-0", rounded === "full" ? "rounded-full" : "rounded-2xl")}
      />
      <div className="min-w-0 flex-1 space-y-3 pb-1">
        <Skeleton className="h-3 w-20" />
        <Skeleton className="h-8 w-72 max-w-full" />
        <Skeleton className="h-4 w-52 max-w-full" />
        <div className="flex items-center gap-3 pt-2">
          <Skeleton className="h-9 w-24 rounded-lg" />
          <Skeleton className="h-9 w-28 rounded-lg" />
          <Skeleton className="h-9 w-9 rounded-lg" />
        </div>
      </div>
    </div>
  );
}

export function TrackListSkeleton({ rows = 6 }: { rows?: number }) {
  return (
    <div role="status" aria-label="Chargement des pistes" className="space-y-1.5">
      <div className="flex items-center justify-between border-b border-white/[0.06] px-3 py-2">
        <Skeleton className="h-3 w-24" />
        <Skeleton className="h-3 w-16" />
      </div>
      {Array.from({ length: rows }, (_, i) => (
        <div key={i} className="flex h-14 items-center gap-3 rounded-xl px-3">
          <Skeleton className="h-4 w-6 shrink-0" />
          <Skeleton className="h-10 w-10 shrink-0 rounded-md" />
          <div className="min-w-0 flex-1 space-y-1.5">
            <Skeleton className="h-3.5 w-48 max-w-[65%]" />
            <Skeleton className="h-3 w-32 max-w-[45%]" />
          </div>
          <Skeleton className="h-4 w-10 shrink-0" />
          <Skeleton className="h-3 w-10 shrink-0" />
        </div>
      ))}
    </div>
  );
}

export function CardGridSkeleton({ count = 6 }: { count?: number }) {
  return (
    <div
      role="status"
      aria-label="Chargement du catalogue"
      className="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6"
    >
      {Array.from({ length: count }, (_, i) => (
        <div key={i} className="card-oled flex flex-col rounded-2xl p-3">
          <Skeleton className="mb-3 aspect-square w-full rounded-xl" />
          <Skeleton className="h-3.5 w-3/4" />
          <Skeleton className="mt-1.5 h-3 w-1/2" />
        </div>
      ))}
    </div>
  );
}
