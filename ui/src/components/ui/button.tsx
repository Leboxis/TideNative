import * as React from "react";
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-lg text-sm font-medium transition-[background-color,color,opacity,transform] duration-150 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-[#33F3FF] disabled:pointer-events-none disabled:opacity-40 active:scale-[0.98] cursor-pointer",
  {
    variants: {
      variant: {
        default: "bg-white text-[#050507] hover:bg-[#e8e8ec] font-semibold",
        secondary:
          "bg-[#16161d] text-[#f5f5f7] border border-white/10 hover:bg-[#1e1e28] hover:border-white/20",
        ghost: "text-white/75 hover:text-white hover:bg-white/[0.08]",
        outline:
          "border border-white/15 bg-transparent text-white hover:bg-white/[0.07] hover:border-white/25",
        destructive:
          "bg-[#ff453a]/15 text-[#ff453a] border border-[#ff453a]/30 hover:bg-[#ff453a]/25",
      },
      size: {
        default: "h-9 px-4 py-2",
        sm: "h-8 rounded-md px-3 text-xs",
        lg: "h-11 rounded-xl px-6 text-sm",
        icon: "h-9 w-9",
        "icon-sm": "h-7 w-7",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  },
);

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean;
}

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, asChild = false, ...props }, ref) => {
    const Comp = asChild ? Slot : "button";
    return (
      <Comp
        className={cn(buttonVariants({ variant, size, className }))}
        ref={ref}
        {...props}
      />
    );
  },
);
Button.displayName = "Button";
