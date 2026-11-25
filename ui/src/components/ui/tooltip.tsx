import * as React from "react";
import * as TooltipPrimitive from "@radix-ui/react-tooltip";
import { cn } from "@/lib/utils";

const TooltipProviderFlag = React.createContext(false);

export function TooltipProvider({
  children,
  ...props
}: React.ComponentPropsWithoutRef<typeof TooltipPrimitive.Provider>) {
  return (
    <TooltipProviderFlag.Provider value={true}>
      <TooltipPrimitive.Provider {...props}>{children}</TooltipPrimitive.Provider>
    </TooltipProviderFlag.Provider>
  );
}

export const Tooltip = TooltipPrimitive.Root;
export const TooltipTrigger = TooltipPrimitive.Trigger;

export const TooltipContent = React.forwardRef<
  React.ComponentRef<typeof TooltipPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof TooltipPrimitive.Content>
>(({ className, sideOffset = 6, ...props }, ref) => (
  <TooltipPrimitive.Portal>
    <TooltipPrimitive.Content
      ref={ref}
      sideOffset={sideOffset}
      className={cn(
        "z-50 inline-flex items-center gap-2 rounded-lg border border-white/15 bg-[#14141d]/95 px-2.5 py-1.5 text-[11px] font-medium text-white shadow-xl backdrop-blur-md select-none",
        className,
      )}
      {...props}
    />
  </TooltipPrimitive.Portal>
));
TooltipContent.displayName = TooltipPrimitive.Content.displayName;

interface ActionTooltipProps {
  label: string;
  shortcut?: string;
  side?: "top" | "bottom" | "left" | "right";
  children: React.ReactElement;
}

export function ActionTooltip({
  label,
  shortcut,
  side = "top",
  children,
}: ActionTooltipProps) {
  const hasProvider = React.useContext(TooltipProviderFlag);
  const content = (
    <Tooltip>
      <TooltipTrigger asChild>{children}</TooltipTrigger>
      <TooltipContent side={side}>
        <span>{label}</span>
        {shortcut ? (
          <kbd className="rounded border border-white/15 bg-white/[0.08] px-1.5 py-0.5 font-mono text-[10px] leading-none text-white/75">
            {shortcut}
          </kbd>
        ) : null}
      </TooltipContent>
    </Tooltip>
  );

  if (hasProvider) {
    return content;
  }
  return <TooltipProvider delayDuration={300}>{content}</TooltipProvider>;
}
