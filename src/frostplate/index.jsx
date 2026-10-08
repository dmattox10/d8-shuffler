import { useEffect, useId, useRef, useState } from "react";
import { createPortal } from "react-dom";

/**
 * The control set. Every one of these is a plain DOM element with the
 * `gc-` classes from theme/controls.css -- no component library, because the
 * look is the point and a library would have to be fought the whole way.
 */

const AURORA = ["--nord14", "--nord8", "--nord13", "--nord15", "--nord12", "--nord7", "--nord9"];

/**
 * A category always gets the same colour, so the eye learns the set.
 *
 * `untagged` is deliberately grey: it is the absence of a category, and
 * colouring it would make unsorted things look sorted.
 */
export function tagColor(name) {
  if (!name || name.toLowerCase() === "untagged") return "var(--nord3)";
  let hash = 0;
  for (let i = 0; i < name.length; i += 1) {
    hash = (hash * 31 + name.charCodeAt(i)) >>> 0;
  }
  return `var(${AURORA[hash % AURORA.length]})`;
}

export function Button({ variant, size, icon, className, children, ...rest }) {
  const classes = ["gc-btn"];
  if (variant) classes.push(`gc-btn--${variant}`);
  if (size) classes.push(`gc-btn--${size}`);
  if (icon) classes.push("gc-btn--icon");
  // Merged, not spread through `rest`: a className in rest would land after
  // this attribute and replace every gc-btn class with it.
  if (className) classes.push(className);
  return (
    <button type="button" className={classes.join(" ")} {...rest}>
      {children}
    </button>
  );
}

export function Segmented({ value, onChange, options, label }) {
  return (
    <div className="gc-seg" role="group" aria-label={label}>
      {options.map((opt) => (
        <button
          key={opt.value}
          type="button"
          className="gc-seg__opt"
          aria-pressed={value === opt.value}
          onClick={() => onChange(opt.value)}
        >
          {opt.label}
        </button>
      ))}
    </div>
  );
}

export function Toggle({ checked, onChange, children }) {
  return (
    <label className="gc-toggle">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <span className="gc-toggle__track">
        <span className="gc-toggle__knob" />
      </span>
      <span className="gc-toggle__label">{children}</span>
    </label>
  );
}

export function Field({ icon, className, children, style }) {
  return (
    <div className={`gc-field${className ? ` ${className}` : ""}`} style={style}>
      {icon ? (
        <span className="gc-field__icon" aria-hidden="true">
          {icon}
        </span>
      ) : null}
      {children}
    </div>
  );
}

export function SearchField({ value, onChange, placeholder = "Search", style }) {
  return (
    <Field icon={<SearchIcon />} style={style}>
      <input
        type="search"
        value={value}
        placeholder={placeholder}
        onChange={(e) => onChange(e.target.value)}
        aria-label={placeholder}
      />
      {value ? (
        <button
          type="button"
          className="gc-chip__x"
          onClick={() => onChange("")}
          aria-label="Clear search"
        >
          &times;
        </button>
      ) : null}
    </Field>
  );
}

export function Panel({ title, subtitle, actions, footer, selected, children }) {
  return (
    <section className={`gc-panel${selected ? " gc-panel--selected" : ""}`} aria-selected={selected || undefined}>
      {title ? (
        <header className="gc-panel__head">
          <h2 className="gc-panel__title">{title}</h2>
          <div style={{ flex: 1 }} />
          {actions}
        </header>
      ) : null}
      <div className="gc-panel__body">
        {subtitle ? <p className="gc-panel__sub">{subtitle}</p> : null}
        {children}
      </div>
      {footer ? <footer className="gc-panel__foot">{footer}</footer> : null}
    </section>
  );
}

export function TagChip({ name, count, pressed, onClick, onRemove }) {
  return (
    <span className="gc-chip" style={{ color: tagColor(name) }} aria-pressed={pressed}
      role={onClick ? "button" : undefined}
      tabIndex={onClick ? 0 : undefined}
      onClick={onClick}
      onKeyDown={(e) => {
        if (onClick && (e.key === "Enter" || e.key === " ")) {
          e.preventDefault();
          onClick();
        }
      }}
    >
      <span>{name}</span>
      {count != null ? <span className="gc-chip__count">{count}</span> : null}
      {onRemove ? (
        <button
          type="button"
          className="gc-chip__x"
          aria-label={`Remove tag ${name}`}
          onClick={(e) => {
            e.stopPropagation();
            onRemove();
          }}
        >
          &times;
        </button>
      ) : null}
    </span>
  );
}

export function Meter({ value, busy, label, tone, striped, tall, segments }) {
  const classes = ["gc-meter"];
  if (busy) classes.push("gc-meter--busy");
  if (tone) classes.push(`gc-meter--${tone}`);
  if (tall) classes.push("gc-meter--tall");

  // A stacked meter is several fills sharing one rail, so the parts read as
  // parts of a whole rather than as separate readings.
  if (segments) {
    return (
      <div className={`${classes.join(" ")} gc-meter--stacked`} role="img" aria-label={label}>
        {segments.map((seg) => (
          <div
            key={seg.label}
            className={`gc-meter__fill${seg.tone ? ` gc-meter--${seg.tone}` : ""}`}
            style={{ width: `${seg.value * 100}%`, background: seg.color }}
            title={seg.label}
          />
        ))}
      </div>
    );
  }

  const fill = ["gc-meter__fill"];
  if (striped) fill.push("gc-meter__fill--striped");
  return (
    <div
      className={classes.join(" ")}
      role="progressbar"
      aria-label={label}
      aria-valuenow={busy ? undefined : Math.round((value ?? 0) * 100)}
    >
      <div className={fill.join(" ")} style={busy ? undefined : { width: `${(value ?? 0) * 100}%` }} />
    </div>
  );
}

export function Alert({ tone, heading, onDismiss, children }) {
  if (!children && !heading) return null;
  const classes = ["gc-alert"];
  // No tone is the danger case, which is what .gc-alert was before it had any.
  if (tone) classes.push(`gc-alert--${tone}`);
  if (onDismiss) classes.push("gc-alert--dismissible");
  return (
    <div className={classes.join(" ")} role="alert">
      {heading ? <h3 className="gc-alert__head">{heading}</h3> : null}
      <div className="gc-alert__body">{children}</div>
      {onDismiss ? (
        <button type="button" className="gc-alert__x" onClick={onDismiss} aria-label="Dismiss">
          <CloseIcon />
        </button>
      ) : null}
    </div>
  );
}

/* ------------------------------------------------------------------ icons -- */
/* Inline so the app ships no icon font and no extra network request. */

const stroke = {
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.8,
  strokeLinecap: "round",
  strokeLinejoin: "round",
};

export function SearchIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true">
      <circle cx="7" cy="7" r="4.5" {...stroke} />
      <path d="M10.5 10.5 14 14" {...stroke} />
    </svg>
  );
}

export function FolderIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true">
      <path d="M1.5 3.5h4l1.5 2h7.5v7h-13z" {...stroke} />
    </svg>
  );
}

export function CubeIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 16 16" aria-hidden="true">
      <path d="M8 1.5 14 5v6l-6 3.5L2 11V5z" {...stroke} />
      <path d="M2 5l6 3.5L14 5M8 8.5V14.5" {...stroke} />
    </svg>
  );
}

export function TrashIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true">
      <path d="M2.5 4h11M6 4V2.5h4V4M4 4l.7 9.5h6.6L12 4" {...stroke} />
    </svg>
  );
}

export function CloseIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true">
      <path d="M4 4l8 8M12 4l-8 8" {...stroke} />
    </svg>
  );
}

export function ExpandIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true">
      <path d="M6 1.5H1.5V6M10 14.5h4.5V10M1.5 10v4.5H6M14.5 6V1.5H10" {...stroke} />
    </svg>
  );
}

export function RefreshIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true">
      <path d="M13.5 8a5.5 5.5 0 1 1-1.7-4" {...stroke} />
      <path d="M13.5 1.5V4.5h-3" {...stroke} />
    </svg>
  );
}

/* ==========================================================================
   The wider component set
   --------------------------------------------------------------------------
   Same contract as above: plain DOM with `gc-` classes, no component library.
   Anything that has to remember whether it is open keeps that state itself,
   because a gallery of forty components should not need forty useStates to
   look at.
   ========================================================================== */

/* ----------------------------------------------------------------- navbar -- */

export function Navbar({ variant, children }) {
  return <nav className={`gc-nav${variant ? ` gc-nav--${variant}` : ""}`}>{children}</nav>;
}

export function NavBrand({ mark, children }) {
  return (
    <span className="gc-nav__brand">
      {mark ? <span className="gc-nav__mark" aria-hidden="true">{mark}</span> : null}
      {children}
    </span>
  );
}

export function NavLinks({ children }) {
  return <ul className="gc-nav__links">{children}</ul>;
}

export function NavLink({ current, disabled, children, ...rest }) {
  return (
    <li>
      <button
        type="button"
        className="gc-nav__link"
        aria-current={current ? "page" : undefined}
        disabled={disabled}
        {...rest}
      >
        {children}
      </button>
    </li>
  );
}

export function NavSpacer() {
  return <span className="gc-nav__spacer" />;
}

/* ------------------------------------------------------ groups and menus -- */

export function ButtonGroup({ vertical, label, children }) {
  return (
    <div className={`gc-btngroup${vertical ? " gc-btngroup--col" : ""}`} role="group" aria-label={label}>
      {children}
    </div>
  );
}

export function Toolbar({ label, children }) {
  return <div className="gc-toolbar" role="toolbar" aria-label={label}>{children}</div>;
}

/**
 * Closes on Escape and on a click anywhere outside it. Both matter: a menu you
 * can only dismiss by picking something from it is a trap.
 */
export function Dropdown({ label, variant, align, children }) {
  const [open, setOpen] = useState(false);
  const root = useRef(null);

  useEffect(() => {
    if (!open) return undefined;
    const away = (e) => {
      if (root.current && !root.current.contains(e.target)) setOpen(false);
    };
    const key = (e) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", away);
    document.addEventListener("keydown", key);
    return () => {
      document.removeEventListener("mousedown", away);
      document.removeEventListener("keydown", key);
    };
  }, [open]);

  return (
    <span className="gc-dropdown" ref={root}>
      <Button variant={variant} aria-expanded={open} aria-haspopup="menu" onClick={() => setOpen((o) => !o)}>
        {label}
        <Caret />
      </Button>
      {open ? (
        <ul className={`gc-menu${align === "end" ? " gc-menu--end" : ""}`} role="menu" onClick={() => setOpen(false)}>
          {children}
        </ul>
      ) : null}
    </span>
  );
}

export function MenuItem({ current, disabled, children, ...rest }) {
  return (
    <li role="none">
      <button type="button" role="menuitem" className="gc-menu__item" aria-current={current ? "true" : undefined} disabled={disabled} {...rest}>
        {children}
      </button>
    </li>
  );
}

export function MenuHeader({ children }) {
  return <li role="none" className="gc-menu__head">{children}</li>;
}

export function MenuSeparator() {
  return <li role="none"><hr className="gc-menu__sep" /></li>;
}

function Caret() {
  return (
    <svg width="8" height="5" viewBox="0 0 8 5" aria-hidden="true" style={{ marginLeft: 2 }}>
      <path d="M0 0h8L4 5z" fill="currentColor" />
    </svg>
  );
}

/* ------------------------------------------------------------------- tabs -- */

export function Tabs({ tabs, value, onChange, variant, label }) {
  const strip = variant === "underline" ? "gc-underline" : "gc-tabs";
  const tab = variant === "underline" ? "gc-underline__tab" : "gc-tabs__tab";
  return (
    <div className={strip} role="tablist" aria-label={label}>
      {tabs.map((t) => (
        <button
          key={t.value}
          type="button"
          role="tab"
          className={tab}
          aria-selected={value === t.value}
          disabled={t.disabled}
          onClick={() => onChange(t.value)}
        >
          {t.label}
        </button>
      ))}
    </div>
  );
}

export function TabPanel({ children }) {
  return <div className="gc-tabpanel" role="tabpanel">{children}</div>;
}

/* ------------------------------------------------- breadcrumb, pagination -- */

export function Breadcrumbs({ items }) {
  return (
    <nav aria-label="Breadcrumb">
      <ol className="gc-crumbs">
        {items.map((item, i) => {
          const last = i === items.length - 1;
          return (
            <li className="gc-crumbs__item" key={item.label} aria-current={last ? "page" : undefined}>
              {last ? item.label : <a href={item.href ?? "#"}>{item.label}</a>}
            </li>
          );
        })}
      </ol>
    </nav>
  );
}

/**
 * Shows every page when there are few, and clamps to a window around the
 * current one when there are many, so the strip never outgrows its row.
 */
export function Pagination({ page, pages, onChange, size, label = "Pagination" }) {
  const window = 5;
  let start = Math.max(1, page - Math.floor(window / 2));
  const end = Math.min(pages, start + window - 1);
  start = Math.max(1, end - window + 1);
  const numbers = [];
  for (let n = start; n <= end; n += 1) numbers.push(n);

  const cls = `gc-pager${size ? ` gc-pager--${size}` : ""}`;
  return (
    <nav aria-label={label}>
      <ul className={cls}>
        <li>
          <button type="button" className="gc-pager__link" disabled={page <= 1} onClick={() => onChange(page - 1)} aria-label="Previous page">&laquo;</button>
        </li>
        {numbers.map((n) => (
          <li key={n}>
            <button
              type="button"
              className="gc-pager__link"
              aria-current={n === page ? "page" : undefined}
              onClick={() => onChange(n)}
            >
              {n}
            </button>
          </li>
        ))}
        <li>
          <button type="button" className="gc-pager__link" disabled={page >= pages} onClick={() => onChange(page + 1)} aria-label="Next page">&raquo;</button>
        </li>
      </ul>
    </nav>
  );
}

/* ------------------------------------------------------------------ table -- */

export function Table({ striped, hover, bordered, size, caption, children }) {
  const classes = ["gc-table"];
  if (striped) classes.push("gc-table--striped");
  if (hover) classes.push("gc-table--hover");
  if (bordered) classes.push("gc-table--bordered");
  if (size) classes.push(`gc-table--${size}`);
  return (
    <div className="gc-tablewrap">
      <table className={classes.join(" ")}>
        {caption ? <caption>{caption}</caption> : null}
        {children}
      </table>
    </div>
  );
}

/* ------------------------------------------------------------- list group -- */

export function ListGroup({ flush, horizontal, children }) {
  const classes = ["gc-list"];
  if (flush) classes.push("gc-list--flush");
  if (horizontal) classes.push("gc-list--row");
  return <ul className={classes.join(" ")}>{children}</ul>;
}

export function ListItem({ action, current, tone, disabled, children, ...rest }) {
  const classes = ["gc-list__item"];
  if (action) classes.push("gc-list__item--action");
  if (tone) classes.push(`gc-list__item--${tone}`);
  const inner = action ? (
    <button
      type="button"
      className={classes.join(" ")}
      aria-current={current ? "true" : undefined}
      disabled={disabled}
      {...rest}
    >
      {children}
    </button>
  ) : (
    <div className={classes.join(" ")} aria-current={current ? "true" : undefined}>{children}</div>
  );
  return <li>{inner}</li>;
}

/* -------------------------------------------------------------- accordion -- */

/** Single-open by default, because two open panels usually means the list
    should have been a set of panels instead. */
export function Accordion({ items, multiple, defaultOpen = [] }) {
  const [open, setOpen] = useState(() => new Set(defaultOpen));

  const toggle = (id) =>
    setOpen((prev) => {
      const next = new Set(multiple ? prev : []);
      if (prev.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  return (
    <div className="gc-acc">
      {items.map((item) => {
        const isOpen = open.has(item.id);
        return (
          <div className="gc-acc__item" key={item.id}>
            <h3 style={{ margin: 0 }}>
              <button type="button" className="gc-acc__btn" aria-expanded={isOpen} onClick={() => toggle(item.id)}>
                {item.title}
                <span className="gc-acc__chevron" aria-hidden="true" />
              </button>
            </h3>
            {isOpen ? <div className="gc-acc__body">{item.body}</div> : null}
          </div>
        );
      })}
    </div>
  );
}

/* --------------------------------------------------------- modal, drawer -- */

/** Escape closes, the scrim closes, and the page behind stops scrolling while
    it is up. Rendered through a portal so a transformed ancestor cannot trap
    a fixed-position dialog. */
function useDismiss(open, onClose) {
  useEffect(() => {
    if (!open) return undefined;
    const key = (e) => {
      if (e.key === "Escape") onClose?.();
    };
    const prev = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    document.addEventListener("keydown", key);
    return () => {
      document.body.style.overflow = prev;
      document.removeEventListener("keydown", key);
    };
  }, [open, onClose]);
}

export function Modal({ open, title, size, footer, onClose, children }) {
  useDismiss(open, onClose);
  if (!open) return null;
  return createPortal(
    <>
      <div className="gc-scrim" onClick={onClose} />
      <div className="gc-modal" role="dialog" aria-modal="true" aria-label={title}>
        <div className={`gc-modal__dialog${size ? ` gc-modal__dialog--${size}` : ""}`}>
          <header className="gc-modal__head">
            <h2 className="gc-modal__title">{title}</h2>
            <button type="button" className="gc-modal__x" onClick={onClose} aria-label="Close">
              <CloseIcon />
            </button>
          </header>
          <div className="gc-modal__body">{children}</div>
          {footer ? <footer className="gc-modal__foot">{footer}</footer> : null}
        </div>
      </div>
    </>,
    document.body
  );
}

export function Drawer({ open, side = "start", title, onClose, children }) {
  useDismiss(open, onClose);
  if (!open) return null;
  return createPortal(
    <>
      <div className="gc-scrim" onClick={onClose} />
      <aside className={`gc-drawer gc-drawer--${side}`} role="dialog" aria-modal="true" aria-label={title}>
        <header className="gc-drawer__head">
          <h2 className="gc-drawer__title">{title}</h2>
          <button type="button" className="gc-drawer__x" onClick={onClose} aria-label="Close">
            <CloseIcon />
          </button>
        </header>
        <div className="gc-drawer__body">{children}</div>
      </aside>
    </>,
    document.body
  );
}

/* ------------------------------------------------------------------ toast -- */

export function ToastStack({ children }) {
  return <div className="gc-toasts" role="region" aria-label="Notifications">{children}</div>;
}

export function Toast({ badge, title, when, onClose, children }) {
  return (
    <div className="gc-toast" role="status" aria-live="polite">
      <header className="gc-toast__head">
        {badge}
        <strong>{title}</strong>
        {when ? <span className="dim">{when}</span> : null}
        {onClose ? (
          <button type="button" className="gc-toast__x" onClick={onClose} aria-label="Dismiss">
            <CloseIcon />
          </button>
        ) : null}
      </header>
      <div className="gc-toast__body">{children}</div>
    </div>
  );
}

/* -------------------------------------------------------- tip and popover -- */

/** Shows on hover and on focus, so it is reachable from the keyboard. */
export function Tooltip({ label, placement = "top", children }) {
  const [shown, setShown] = useState(false);
  const id = useId();
  return (
    <span
      className="gc-tipwrap"
      onMouseEnter={() => setShown(true)}
      onMouseLeave={() => setShown(false)}
      onFocus={() => setShown(true)}
      onBlur={() => setShown(false)}
      aria-describedby={shown ? id : undefined}
    >
      {children}
      {shown ? <span className={`gc-tip gc-tip--${placement}`} role="tooltip" id={id}>{label}</span> : null}
    </span>
  );
}

export function Popover({ title, content, placement = "top", children }) {
  const [open, setOpen] = useState(false);
  const root = useRef(null);

  useEffect(() => {
    if (!open) return undefined;
    const away = (e) => {
      if (root.current && !root.current.contains(e.target)) setOpen(false);
    };
    const key = (e) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", away);
    document.addEventListener("keydown", key);
    return () => {
      document.removeEventListener("mousedown", away);
      document.removeEventListener("keydown", key);
    };
  }, [open]);

  return (
    <span className="gc-tipwrap" ref={root}>
      <span onClick={() => setOpen((o) => !o)}>{children}</span>
      {open ? (
        <div className={`gc-pop gc-pop--${placement}`} role="dialog" aria-label={title}>
          {title ? <h3 className="gc-pop__head">{title}</h3> : null}
          <div className="gc-pop__body">{content}</div>
        </div>
      ) : null}
    </span>
  );
}

/* ------------------------------------------------------------------ badge -- */

export function Badge({ tone, pill, children }) {
  const classes = ["gc-badge"];
  if (tone) classes.push(`gc-badge--${tone}`);
  if (pill) classes.push("gc-badge--pill");
  return <span className={classes.join(" ")}>{children}</span>;
}

/* ------------------------------------------------------------------ forms -- */

export function Label({ htmlFor, children }) {
  return <label className="gc-label" htmlFor={htmlFor}>{children}</label>;
}

export function Help({ tone, children }) {
  return <span className={`gc-help${tone ? ` gc-help--${tone}` : ""}`}>{children}</span>;
}

export function FormRow({ label, help, helpTone, htmlFor, children }) {
  return (
    <div className="gc-formrow">
      {label ? <Label htmlFor={htmlFor}>{label}</Label> : null}
      {children}
      {help ? <Help tone={helpTone}>{help}</Help> : null}
    </div>
  );
}

export function Textarea({ ...rest }) {
  return <textarea className="gc-textarea" {...rest} />;
}

export function Checkbox({ checked, onChange, disabled, children }) {
  return (
    <label className="gc-check">
      <input type="checkbox" checked={checked} disabled={disabled} onChange={(e) => onChange?.(e.target.checked)} />
      <span>{children}</span>
    </label>
  );
}

export function Radio({ name, value, checked, onChange, disabled, children }) {
  return (
    <label className="gc-check">
      <input type="radio" name={name} value={value} checked={checked} disabled={disabled} onChange={() => onChange?.(value)} />
      <span>{children}</span>
    </label>
  );
}

export function InputGroup({ children }) {
  return <div className="gc-group">{children}</div>;
}

export function InputCap({ children }) {
  return <span className="gc-group__cap">{children}</span>;
}

/** The label lives inside the well until there is something to read, then
    rises out of the way rather than vanishing. */
export function FloatingField({ label, value, onChange, type = "text" }) {
  const [focused, setFocused] = useState(false);
  const id = useId();
  const lifted = focused || (value ?? "") !== "";
  return (
    <div className={`gc-float${lifted ? " gc-float--lifted" : ""}`}>
      <label className="gc-float__label" htmlFor={id}>{label}</label>
      <div className="gc-field">
        <input
          id={id}
          type={type}
          value={value}
          onChange={(e) => onChange?.(e.target.value)}
          onFocus={() => setFocused(true)}
          onBlur={() => setFocused(false)}
        />
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ prose -- */

export function Prose({ children }) {
  return <div className="gc-prose">{children}</div>;
}
