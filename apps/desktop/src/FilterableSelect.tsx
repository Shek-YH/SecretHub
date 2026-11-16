import { useState } from 'react';

/** Above this many candidates a native <select> is replaced by a searchable control. */
export const SEARCH_THRESHOLD = 15;
const MAX_VISIBLE = 100;

export type FilterableGroup = { label?: string; options: Array<{ id: string; text: string }> };

/**
 * Text input with a filtered suggestion list. The input value is the chosen id itself,
 * so a custom id can be typed directly; suggestions only help to pick or narrow down.
 */
export function FilterableSelect({ ariaLabel, value, onChange, groups, placeholder, truncatedLabel, noMatchLabel }: {
  ariaLabel: string;
  value: string;
  onChange: (id: string) => void;
  groups: FilterableGroup[];
  placeholder?: string;
  /** Contains "{n}"; shown when more matches exist than are listed. */
  truncatedLabel?: string;
  noMatchLabel?: string;
}) {
  const [open, setOpen] = useState(false);
  const [typed, setTyped] = useState(false);
  const needle = typed ? value.trim().toLowerCase() : '';
  const filtered = groups
    .map((group) => ({ ...group, options: group.options.filter((option) => !needle || `${option.id} ${option.text}`.toLowerCase().includes(needle)) }))
    .filter((group) => group.options.length > 0);
  const total = filtered.reduce((sum, group) => sum + group.options.length, 0);
  let budget = MAX_VISIBLE;
  const visible = filtered
    .map((group) => {
      const options = group.options.slice(0, Math.max(0, budget));
      budget -= options.length;
      return { ...group, options };
    })
    .filter((group) => group.options.length > 0);

  return (
    <div className="filterable-select">
      <input
        role="combobox"
        aria-label={ariaLabel}
        aria-expanded={open}
        aria-autocomplete="list"
        autoComplete="off"
        value={value}
        placeholder={placeholder}
        onFocus={() => setOpen(true)}
        onBlur={() => { setOpen(false); setTyped(false); }}
        onKeyDown={(event) => { if (event.key === 'Escape') setOpen(false); }}
        onChange={(event) => { setTyped(true); setOpen(true); onChange(event.target.value); }}
      />
      {open && (
        <div className="filterable-list" role="listbox">
          {visible.map((group, index) => (
            <div key={group.label ?? index} role="group" aria-label={group.label}>
              {group.label && <div className="filterable-group-label">{group.label}</div>}
              {group.options.map((option) => (
                <div
                  key={option.id}
                  role="option"
                  aria-selected={option.id === value}
                  className="filterable-option"
                  // mousedown keeps focus in the input so blur does not close the list before the pick lands
                  onMouseDown={(event) => { event.preventDefault(); onChange(option.id); setTyped(false); setOpen(false); }}
                >
                  {option.text}
                </div>
              ))}
            </div>
          ))}
          {total === 0 && noMatchLabel && <div className="filterable-note">{noMatchLabel}</div>}
          {total > MAX_VISIBLE && truncatedLabel && <div className="filterable-note">{truncatedLabel.replace('{n}', String(MAX_VISIBLE))}</div>}
        </div>
      )}
    </div>
  );
}
