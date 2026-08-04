// Adapted from Amicro's GlowButton (MIT). See THIRD_PARTY_NOTICES.md.
import { LoaderCircle } from "lucide-react";
import {
  forwardRef,
  type ButtonHTMLAttributes,
  type PointerEventHandler,
  type ReactNode,
} from "react";

type MicroButtonTone = "primary" | "secondary" | "danger" | "warning";

interface MicroButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  tone?: MicroButtonTone;
  icon?: ReactNode;
  loading?: boolean;
  loadingLabel?: ReactNode;
}

export const MicroButton = forwardRef<HTMLButtonElement, MicroButtonProps>(function MicroButton({
  tone = "primary",
  icon,
  loading = false,
  loadingLabel = "正在处理...",
  className = "",
  disabled,
  children,
  onPointerMove,
  ...props
}, ref) {
  const updateGlow: PointerEventHandler<HTMLButtonElement> = (event) => {
    const bounds = event.currentTarget.getBoundingClientRect();
    event.currentTarget.style.setProperty("--micro-pointer-x", `${event.clientX - bounds.left}px`);
    event.currentTarget.style.setProperty("--micro-pointer-y", `${event.clientY - bounds.top}px`);
    onPointerMove?.(event);
  };

  return (
    <button
      {...props}
      ref={ref}
      className={`button micro-button button-${tone}${className ? ` ${className}` : ""}`}
      disabled={disabled || loading}
      aria-busy={loading || undefined}
      onPointerMove={updateGlow}
    >
      <span className="micro-button-glow" aria-hidden="true" />
      <span className="micro-button-content">
        {(loading || icon) && (
          <span className="micro-button-icon" aria-hidden="true">
            {loading ? <LoaderCircle className="micro-button-spinner" size={15} /> : icon}
          </span>
        )}
        <span className="micro-button-label">{loading ? loadingLabel : children}</span>
      </span>
    </button>
  );
});
