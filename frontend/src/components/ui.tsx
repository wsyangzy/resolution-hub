import * as AlertDialogPrimitive from "@radix-ui/react-alert-dialog";
import * as DropdownMenuPrimitive from "@radix-ui/react-dropdown-menu";
import * as SwitchPrimitive from "@radix-ui/react-switch";
import * as TabsPrimitive from "@radix-ui/react-tabs";
import { Check, ChevronDown, X } from "lucide-react";
import {
    type ReactNode,
    useEffect,
    useLayoutEffect,
    useRef,
    useState,
} from "react";

import { MicroButton } from "./MicroButton";

export interface ConfirmOptions {
    title: string;
    description: string;
    confirmLabel: string;
    tone?: "default" | "danger" | "warning";
    confirmIcon?: ReactNode;
    choices?: ConfirmChoice[];
    choiceDefault?: string;
    showDontAskAgain?: boolean;
    hideCancel?: boolean;
    returnFocus?: HTMLElement | null;
    onConfirm: (result?: ConfirmResult) => Promise<void> | void;
}

export interface ConfirmChoice {
    value: string;
    label: string;
    detail?: string;
    icon?: ReactNode;
    tone?: "default" | "danger";
}

export interface ConfirmResult {
    choice?: string;
    dontAskAgain: boolean;
}

interface ConfirmDialogProps {
    options: ConfirmOptions | null;
    onClose: () => void;
    onError: (error: unknown) => void;
}

export function ConfirmDialog({ options, onClose, onError }: ConfirmDialogProps) {
    const [busy, setBusy] = useState(false);
    const [inlineError, setInlineError] = useState<string | null>(null);
    const [selectedChoice, setSelectedChoice] = useState(
        options?.choiceDefault ?? options?.choices?.[0]?.value ?? "",
    );
    const [dontAskAgain, setDontAskAgain] = useState(false);
    const [visualState, setVisualState] = useState<"open" | "closing" | "closed">("closed");
    const renderedOptions = useRef<ConfirmOptions | null>(options);

    if (options) {
        renderedOptions.current = options;
    }

    useEffect(() => {
        if (options) {
            setInlineError(null);
            setSelectedChoice(options.choiceDefault ?? options.choices?.[0]?.value ?? "");
            setDontAskAgain(false);
            setVisualState("open");
        } else {
            setVisualState((state) => state === "open" ? "closing" : state);
        }
    }, [options]);

    useEffect(() => {
        if (visualState !== "closing") {
            return;
        }
        const styles = getComputedStyle(document.documentElement);
        const duration = Number.parseFloat(styles.getPropertyValue("--modal-close-dur")) || 150;
        const timeout = window.setTimeout(() => setVisualState("closed"), duration);
        return () => window.clearTimeout(timeout);
    }, [visualState]);

    const runAction = async () => {
        if (!options || busy) {
            return;
        }
        setBusy(true);
        try {
            await options.onConfirm({ choice: selectedChoice || undefined, dontAskAgain });
            onClose();
        } catch (error) {
            setInlineError(
                typeof error === "string"
                    ? error
                    : error instanceof Error && error.message
                        ? error.message
                        : "操作失败，请稍后重试",
            );
            onError(error);
        } finally {
            setBusy(false);
        }
    };

    const visible = visualState !== "closed";
    const currentOptions = options ?? renderedOptions.current;

    if (!visible && !options) {
        return null;
    }

    return (
        <AlertDialogPrimitive.Root open={Boolean(options)} onOpenChange={(open) => {
            if (!open && !busy) {
                onClose();
            }
        }}>
            <AlertDialogPrimitive.Portal forceMount>
                <AlertDialogPrimitive.Overlay
                    forceMount
                    className={`confirm-overlay${visible ? " is-visible" : ""}`}
                />
                <AlertDialogPrimitive.Content
                    forceMount
                    className={`confirm-dialog t-modal${visualState === "open" ? " is-open" : ""}${visualState === "closing" ? " is-closing" : ""}`}
                    aria-hidden={!visible}
                    onCloseAutoFocus={(event) => {
                        const target = currentOptions?.returnFocus;
                        if (target?.isConnected) {
                            event.preventDefault();
                            target.focus();
                        }
                    }}
                >
                    <div className={`confirm-mark confirm-mark-${currentOptions?.tone ?? "default"}`} aria-hidden="true">
                        !
                    </div>
                    <AlertDialogPrimitive.Title className="confirm-title">
                        {currentOptions?.title ?? "确认操作"}
                    </AlertDialogPrimitive.Title>
                    <AlertDialogPrimitive.Description className="confirm-description">
                        {currentOptions?.description ?? ""}
                    </AlertDialogPrimitive.Description>
                    {currentOptions?.choices && currentOptions.choices.length > 0 && (
                        <div className="confirm-choice-group" role="radiogroup" aria-label="关闭主窗口后的处理方式">
                            {currentOptions.choices.map((choice) => (
                                <button
                                    key={choice.value}
                                    type="button"
                                    role="radio"
                                    aria-checked={selectedChoice === choice.value}
                                    className={`confirm-choice${selectedChoice === choice.value ? " is-selected" : ""}${choice.tone === "danger" ? " is-danger" : ""}`}
                                    disabled={busy}
                                    onClick={() => setSelectedChoice(choice.value)}
                                >
                                    <span className="confirm-choice-radio" aria-hidden="true"><span /></span>
                                    {choice.icon && <span className="confirm-choice-icon" aria-hidden="true">{choice.icon}</span>}
                                    <span className="confirm-choice-copy">
                                        <strong>{choice.label}</strong>
                                        {choice.detail && <small>{choice.detail}</small>}
                                    </span>
                                </button>
                            ))}
                        </div>
                    )}
                    {currentOptions?.showDontAskAgain && (
                        <div className="confirm-checkbox-option">
                            <MotionCheckbox
                                checked={dontAskAgain}
                                disabled={busy}
                                onCheckedChange={setDontAskAgain}
                                label="不再提醒我"
                            />
                        </div>
                    )}
                    {inlineError && <p className="confirm-error" role="alert">{inlineError}</p>}
                    <div className={`confirm-actions${currentOptions?.hideCancel ? " is-single" : ""}`}>
                        {!currentOptions?.hideCancel && (
                            <AlertDialogPrimitive.Cancel asChild>
                                <MicroButton type="button" tone="secondary" icon={<X size={15} />} disabled={busy}>
                                    取消
                                </MicroButton>
                            </AlertDialogPrimitive.Cancel>
                        )}
                        <MicroButton
                            type="button"
                            tone={currentOptions?.tone === "danger"
                                ? "danger"
                                : currentOptions?.tone === "warning"
                                    ? "warning"
                                    : "primary"}
                            icon={currentOptions?.confirmIcon}
                            loading={busy}
                            onClick={() => void runAction()}
                            disabled={busy}
                        >
                            {currentOptions?.confirmLabel ?? "确认"}
                        </MicroButton>
                    </div>
                </AlertDialogPrimitive.Content>
            </AlertDialogPrimitive.Portal>
        </AlertDialogPrimitive.Root>
    );
}

interface ToastProps {
    message: string | null;
    tone?: "error" | "success" | "info";
    onClose: () => void;
}

export function Toast({ message, tone = "error", onClose }: ToastProps) {
    const [renderedMessage, setRenderedMessage] = useState<string | null>(message);
    const [open, setOpen] = useState(Boolean(message));

    useEffect(() => {
        if (!message || tone === "error") {
            return;
        }
        const timeout = window.setTimeout(onClose, 3200);
        return () => window.clearTimeout(timeout);
    }, [message, onClose, tone]);

    useEffect(() => {
        if (message) {
            setRenderedMessage(message);
            setOpen(true);
            return;
        }
        if (!renderedMessage) {
            return;
        }
        setOpen(false);
        const styles = getComputedStyle(document.documentElement);
        const duration = Number.parseFloat(styles.getPropertyValue("--toast-close")) || 250;
        const timeout = window.setTimeout(() => setRenderedMessage(null), duration);
        return () => window.clearTimeout(timeout);
    }, [message, renderedMessage]);

    if (!renderedMessage) {
        return null;
    }

    return (
        <div
            className={`app-toast t-toast toast-${tone}${open ? " is-open" : ""}`}
            role={tone === "error" ? "alert" : "status"}
            aria-hidden={!open}
        >
            <span>{renderedMessage}</span>
            <button type="button" className="icon-button compact" onClick={onClose} aria-label="关闭提示">
                <X size={15} />
            </button>
        </div>
    );
}

interface TooltipProps {
    label: string;
    children: ReactNode;
    side?: "top" | "right";
}

export function Tooltip({ label, children, side = "top" }: TooltipProps) {
    const id = `tooltip-${label.replace(/\s+/g, "-")}`;
    const [dismissed, setDismissed] = useState(false);
    return (
        <span
            className={`t-tt-wrap tooltip-${side}`}
            data-dismissed={dismissed}
            onClick={() => setDismissed(true)}
            onPointerLeave={() => setDismissed(false)}
        >
            <span className="t-tt-trigger" aria-describedby={id}>{children}</span>
            <span className="t-tt" id={id} role="tooltip">{label}</span>
        </span>
    );
}

export interface SelectOption<T extends string> {
    value: T;
    label: string;
    detail?: string;
    disabled?: boolean;
}

interface SelectControlProps<T extends string> {
    id: string;
    value: T | "";
    options: Array<SelectOption<T>>;
    onValueChange: (value: T) => void;
    placeholder: string;
    disabled?: boolean;
    required?: boolean;
}

export function SelectControl<T extends string>({
    id,
    value,
    options,
    onValueChange,
    placeholder,
    disabled = false,
    required = false,
}: SelectControlProps<T>) {
    const selected = options.find((option) => option.value === value);
    const unavailable = disabled || options.length === 0;

    return (
        <DropdownMenuPrimitive.Root modal={false}>
            <DropdownMenuPrimitive.Trigger asChild>
                <button
                    id={id}
                    type="button"
                    className="select-control-trigger"
                    disabled={unavailable}
                    aria-required={required || undefined}
                >
                    <span className={`select-control-value${selected ? "" : " is-placeholder"}`}>
                        <span className="select-control-label">{selected?.label ?? placeholder}</span>
                        {selected?.detail && <span className="select-control-detail">{selected.detail}</span>}
                    </span>
                    <ChevronDown className="select-control-chevron" size={16} aria-hidden="true" />
                </button>
            </DropdownMenuPrimitive.Trigger>
            <DropdownMenuPrimitive.Portal>
                <DropdownMenuPrimitive.Content
                    className="select-control-content"
                    align="start"
                    sideOffset={6}
                    collisionPadding={12}
                >
                    <DropdownMenuPrimitive.RadioGroup
                        value={value}
                        onValueChange={(next) => onValueChange(next as T)}
                    >
                        {options.map((option) => (
                            <DropdownMenuPrimitive.RadioItem
                                key={option.value}
                                className="select-control-item"
                                value={option.value}
                                textValue={`${option.label} ${option.detail ?? ""}`.trim()}
                                aria-label={option.detail ? `${option.label}，${option.detail}` : option.label}
                                disabled={option.disabled}
                            >
                                <span className="select-option-copy">
                                    <span className="select-option-label">{option.label}</span>
                                    {option.detail && <span className="select-option-detail">{option.detail}</span>}
                                </span>
                                <DropdownMenuPrimitive.ItemIndicator className="select-option-indicator">
                                    <Check size={14} aria-hidden="true" />
                                </DropdownMenuPrimitive.ItemIndicator>
                            </DropdownMenuPrimitive.RadioItem>
                        ))}
                    </DropdownMenuPrimitive.RadioGroup>
                </DropdownMenuPrimitive.Content>
            </DropdownMenuPrimitive.Portal>
        </DropdownMenuPrimitive.Root>
    );
}

export interface TabOption<T extends string> {
    value: T;
    label: string;
    icon?: ReactNode;
}

interface SlidingTabsProps<T extends string> {
    value: T;
    options: Array<TabOption<T>>;
    onValueChange: (value: T) => void;
    children?: ReactNode;
    ariaLabel: string;
    className?: string;
}

export function SlidingTabs<T extends string>({
    value,
    options,
    onValueChange,
    children,
    ariaLabel,
    className = "",
}: SlidingTabsProps<T>) {
    const listRef = useRef<HTMLDivElement>(null);
    const pillRef = useRef<HTMLSpanElement>(null);

    const movePill = (animate: boolean) => {
        const list = listRef.current;
        const pill = pillRef.current;
        const active = list?.querySelector<HTMLElement>(`[data-tab-value="${CSS.escape(value)}"]`);
        if (!list || !pill || !active) {
            return;
        }
        const previous = pill.style.transition;
        if (!animate) {
            pill.style.transition = "none";
        }
        pill.style.transform = `translateX(${active.offsetLeft}px)`;
        pill.style.width = `${active.offsetWidth}px`;
        if (!animate) {
            void pill.offsetWidth;
            pill.style.transition = previous;
        }
    };

    useLayoutEffect(() => movePill(false), []);
    useLayoutEffect(() => movePill(true), [value]);
    useEffect(() => {
        const onResize = () => movePill(false);
        window.addEventListener("resize", onResize);
        return () => window.removeEventListener("resize", onResize);
    }, [value]);

    return (
        <TabsPrimitive.Root value={value} onValueChange={(next) => onValueChange(next as T)} className={className}>
            <TabsPrimitive.List ref={listRef} className="t-tabs" aria-label={ariaLabel}>
                <span ref={pillRef} className="t-tabs-pill" aria-hidden="true" />
                {options.map((option) => (
                    <TabsPrimitive.Trigger
                        key={option.value}
                        value={option.value}
                        data-tab-value={option.value}
                        className="t-tab"
                    >
                        {option.icon}
                        <span>{option.label}</span>
                    </TabsPrimitive.Trigger>
                ))}
            </TabsPrimitive.List>
            {children}
        </TabsPrimitive.Root>
    );
}

interface AccordionProps {
    title: ReactNode;
    children: ReactNode;
    defaultOpen?: boolean;
    className?: string;
}

export function Accordion({ title, children, defaultOpen = false, className = "" }: AccordionProps) {
    const [open, setOpen] = useState(defaultOpen);
    return (
        <section className={`t-acc ${className}`} data-open={open}>
            <button type="button" className="t-acc-head" aria-expanded={open} onClick={() => setOpen((value) => !value)}>
                <span>{title}</span>
                <span className="t-acc-chevron" aria-hidden="true">
                    <ChevronDown size={16} />
                </span>
            </button>
            <div className="t-acc-panel" aria-hidden={!open} inert={!open}>
                <div className="t-acc-panel-inner">{children}</div>
            </div>
        </section>
    );
}

interface MotionSwitchProps {
    checked: boolean;
    disabled?: boolean;
    onCheckedChange: (checked: boolean) => void;
    ariaLabel: string;
}

interface MotionCheckboxProps {
    checked: boolean;
    disabled?: boolean;
    onCheckedChange: (checked: boolean) => void;
    label: string;
}

export function MotionCheckbox({ checked, disabled, onCheckedChange, label }: MotionCheckboxProps) {
    return (
        <button
            type="button"
            className="t-check motion-checkbox"
            role="checkbox"
            aria-checked={checked}
            disabled={disabled}
            onClick={() => onCheckedChange(!checked)}
        >
            <span className="motion-checkbox-box" aria-hidden="true">
                <svg viewBox="0 0 10.1668 10.1668">
                    <path d="M1 5.52L3.92 9.17L9.17 1" />
                </svg>
            </span>
            <span>{label}</span>
        </button>
    );
}

export function MotionSwitch({ checked, disabled, onCheckedChange, ariaLabel }: MotionSwitchProps) {
    const [initialized, setInitialized] = useState(false);
    return (
        <SwitchPrimitive.Root
            className={`t-toggle${initialized ? " is-init" : ""}`}
            checked={checked}
            disabled={disabled}
            data-on={checked}
            aria-label={ariaLabel}
            onCheckedChange={(next) => {
                setInitialized(true);
                onCheckedChange(next);
            }}
        >
            <SwitchPrimitive.Thumb className="t-toggle-thumb" />
        </SwitchPrimitive.Root>
    );
}

export function RuntimeText({ children }: { children: string }) {
    const [text, setText] = useState(children);
    const [phase, setPhase] = useState<"idle" | "exit" | "enter">("idle");

    useEffect(() => {
        if (children === text) {
            return;
        }
        if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
            setText(children);
            return;
        }
        setPhase("exit");
        const duration = Number.parseFloat(
            getComputedStyle(document.documentElement).getPropertyValue("--text-swap-dur"),
        ) || 150;
        const timeout = window.setTimeout(() => {
            setText(children);
            setPhase("enter");
            window.requestAnimationFrame(() => setPhase("idle"));
        }, duration);
        return () => window.clearTimeout(timeout);
    }, [children, text]);

    return (
        <span className={`t-text-swap${phase === "exit" ? " is-exit" : ""}${phase === "enter" ? " is-enter-start" : ""}`}>
            {text}
        </span>
    );
}
