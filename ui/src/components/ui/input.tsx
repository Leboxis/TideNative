import * as React from "react";
import { cn } from "@/lib/utils";

export const Input = React.forwardRef<HTMLInputElement, React.InputHTMLAttributes<HTMLInputElement>>(
  ({ className, type, ...props }, ref) => (
    <input
      type={type}
      ref={ref}
      className={cn(
        "flex h-9 w-full rounded-lg border border-white/12 bg-[#111116] px-3 py-1 text-sm text-[#f5f5f7] placeholder:text-white/35 focus-visible:outline-none focus-visible:border-[#33F3FF]/70 disabled:cursor-not-allowed disabled:opacity-40 transition-colors duration-150",
        className,
      )}
      {...props}
    />
  ),
);
Input.displayName = "Input";
