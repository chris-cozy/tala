import { forwardRef, type ButtonHTMLAttributes, type ReactNode } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import * as Dropdown from "@radix-ui/react-dropdown-menu";
import * as Tooltip from "@radix-ui/react-tooltip";
import {
  X,
  ChevronDown,
  MoreHorizontal,
  LoaderCircle,
  Sparkles,
} from "lucide-react";

export const Button = forwardRef<
  HTMLButtonElement,
  ButtonHTMLAttributes<HTMLButtonElement> & {
    variant?: "primary" | "secondary" | "ghost" | "danger";
    busy?: boolean;
  }
>(function Button(
  { variant = "secondary", busy, className = "", children, disabled, ...props },
  ref,
) {
  return (
    <button
      ref={ref}
      type="button"
      className={`button ${variant} ${className}`}
      disabled={disabled || busy}
      {...props}
    >
      {busy && <LoaderCircle className="spin" size={16} />} {children}
    </button>
  );
});
export function IconButton({
  label,
  children,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { label: string }) {
  return (
    <Tooltip.Root>
      <Tooltip.Trigger asChild>
        <button
          type="button"
          aria-label={label}
          className="icon-button"
          {...props}
        >
          {children}
        </button>
      </Tooltip.Trigger>
      <Tooltip.Portal>
        <Tooltip.Content className="tooltip" sideOffset={8}>
          {label}
        </Tooltip.Content>
      </Tooltip.Portal>
    </Tooltip.Root>
  );
}
export function Modal({
  title,
  description,
  open,
  onClose,
  children,
  wide = false,
}: {
  title: string;
  description?: string;
  open: boolean;
  onClose: () => void;
  children: ReactNode;
  wide?: boolean;
}) {
  return (
    <Dialog.Root open={open} onOpenChange={(value) => !value && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className="modal-overlay" />
        <Dialog.Content
          className={`modal ${wide ? "wide" : ""}`}
          aria-describedby={description ? "modal-description" : undefined}
        >
          <div className="modal-heading">
            <div>
              <Dialog.Title>{title}</Dialog.Title>
              {description && (
                <Dialog.Description id="modal-description">
                  {description}
                </Dialog.Description>
              )}
            </div>
            <Dialog.Close asChild>
              <button className="icon-button" aria-label="Close dialog">
                <X size={19} />
              </button>
            </Dialog.Close>
          </div>
          {children}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
export type MenuItem = {
  label: string;
  action: () => void;
  icon?: ReactNode;
  danger?: boolean;
  disabled?: boolean;
  separator?: boolean;
};
export function Menu({
  items,
  label = "More options",
  children,
}: {
  items: MenuItem[];
  label?: string;
  children?: ReactNode;
}) {
  return (
    <Dropdown.Root>
      <Dropdown.Trigger asChild>
        {children ? (
          <Button>
            {children}
            <ChevronDown size={13} />
          </Button>
        ) : (
          <button type="button" className="icon-button" aria-label={label}>
            <MoreHorizontal size={20} />
          </button>
        )}
      </Dropdown.Trigger>
      <Dropdown.Portal>
        <Dropdown.Content className="dropdown" align="end" sideOffset={8}>
          {items.map((item, i) => (
            <div key={i}>
              {item.separator && (
                <Dropdown.Separator className="menu-separator" />
              )}
              <Dropdown.Item
                className={`menu-item ${item.danger ? "danger-text" : ""}`}
                onSelect={item.action}
                disabled={item.disabled}
              >
                {item.icon}
                {item.label}
              </Dropdown.Item>
            </div>
          ))}
        </Dropdown.Content>
      </Dropdown.Portal>
    </Dropdown.Root>
  );
}
export function Empty({
  title,
  children,
  action,
  icon,
}: {
  title: string;
  children?: ReactNode;
  action?: ReactNode;
  icon?: ReactNode;
}) {
  return (
    <div className="empty-state">
      <div className="empty-icon">{icon ?? <Sparkles size={28} />}</div>
      <h2>{title}</h2>
      <p>{children}</p>
      {action}
    </div>
  );
}
export function PageHeader({
  eyebrow,
  title,
  subtitle,
  actions,
}: {
  eyebrow?: string;
  title: string;
  subtitle?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <header className="page-header">
      <div>
        {eyebrow && <div className="eyebrow">{eyebrow}</div>}
        <h1>{title}</h1>
        {subtitle && <p>{subtitle}</p>}
      </div>
      <div className="header-actions">{actions}</div>
    </header>
  );
}
export function Progress({ value, label }: { value: number; label: string }) {
  return (
    <div
      className="progress-track"
      role="progressbar"
      aria-label={label}
      aria-valuenow={Math.round(Math.min(1, value) * 100)}
      aria-valuemin={0}
      aria-valuemax={100}
    >
      <span style={{ width: `${Math.min(1, Math.max(0, value)) * 100}%` }} />
    </div>
  );
}
export function Loading({
  label = "Opening your collection…",
}: {
  label?: string;
}) {
  return (
    <div className="loading">
      <LoaderCircle className="spin" size={22} />
      <span>{label}</span>
    </div>
  );
}
export function ErrorPanel({
  message,
  retry,
}: {
  message: string;
  retry?: () => void;
}) {
  return (
    <div className="error-panel" role="alert">
      <strong>We couldn’t complete that</strong>
      <p>{message}</p>
      {retry && <Button onClick={retry}>Try again</Button>}
    </div>
  );
}
