import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

const badgeVariants = cva(
  "inline-flex items-center rounded px-1.5 py-0.5 text-[10px] font-bold tracking-wider uppercase leading-none select-none",
  {
    variants: {
      variant: {
        max: "bg-[#FFD432]/15 text-[#FFD432] border border-[#FFD432]/35",
        high: "bg-[#33F3FF]/12 text-[#33F3FF] border border-[#33F3FF]/30",
        low: "bg-white/10 text-[#A8A8B3] border border-white/15",
        bitperfect: "bg-[#30D158]/15 text-[#30D158] border border-[#30D158]/35",
        explicit: "bg-white/12 text-white/75 border border-white/15 px-1",
        subtle: "bg-white/[0.06] text-white/70 border border-white/10",
      },
    },
    defaultVariants: {
      variant: "subtle",
    },
  },
);

export interface BadgeProps
  extends React.HTMLAttributes<HTMLSpanElement>,
    VariantProps<typeof badgeVariants> {}

export function Badge({ className, variant, ...props }: BadgeProps) {
  return <span className={cn(badgeVariants({ variant }), className)} {...props} />;
}
