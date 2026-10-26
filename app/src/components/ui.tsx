import { Loader2 } from 'lucide-react'
import type { ButtonHTMLAttributes, InputHTMLAttributes, ReactNode, SelectHTMLAttributes } from 'react'
import { useId } from 'react'

export function cx(...parts: (string | false | null | undefined)[]): string {
  return parts.filter(Boolean).join(' ')
}

export function Spinner({ className = 'size-4' }: { className?: string }) {
  return <Loader2 className={cx('animate-spin', className)} aria-hidden />
}

export function Card({ title, subtitle, action, children, className }: { title?: ReactNode; subtitle?: ReactNode; action?: ReactNode; children: ReactNode; className?: string }) {
  return (
    <section className={cx('rounded-2xl border border-slate-200 bg-white p-5 shadow-sm dark:border-slate-800 dark:bg-slate-900', className)}>
      {(title || action) && (
        <header className="mb-3 flex items-start justify-between gap-3">
          <div>
            {title && <h2 className="text-sm font-semibold text-slate-900 dark:text-slate-100">{title}</h2>}
            {subtitle && <p className="mt-0.5 text-xs text-slate-500 dark:text-slate-400">{subtitle}</p>}
          </div>
          {action}
        </header>
      )}
      {children}
    </section>
  )
}

type Tone = 'neutral' | 'good' | 'warn' | 'bad' | 'brand'
const TONES: Record<Tone, string> = {
  neutral: 'bg-slate-100 text-slate-700 dark:bg-slate-800 dark:text-slate-300',
  good: 'bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-300',
  warn: 'bg-amber-100 text-amber-800 dark:bg-amber-950 dark:text-amber-300',
  bad: 'bg-rose-100 text-rose-800 dark:bg-rose-950 dark:text-rose-300',
  brand: 'bg-brand-100 text-brand-700 dark:bg-indigo-950 dark:text-indigo-300',
}

export function Badge({ tone = 'neutral', children }: { tone?: Tone; children: ReactNode }) {
  return <span className={cx('inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-xs font-medium', TONES[tone])}>{children}</span>
}

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'primary' | 'secondary' | 'danger' | 'ghost'
  loading?: boolean
  icon?: ReactNode
}

export function Button({ variant = 'secondary', loading, icon, children, className, disabled, ...rest }: ButtonProps) {
  const styles = {
    primary: 'bg-brand-600 text-white hover:bg-brand-700 disabled:bg-brand-600/50',
    secondary: 'border border-slate-300 bg-white text-slate-800 hover:bg-slate-50 disabled:opacity-50 dark:border-slate-700 dark:bg-slate-900 dark:text-slate-100 dark:hover:bg-slate-800',
    danger: 'border border-rose-300 text-rose-700 hover:bg-rose-50 disabled:opacity-50 dark:border-rose-900 dark:text-rose-300 dark:hover:bg-rose-950',
    ghost: 'text-slate-600 hover:bg-slate-100 disabled:opacity-50 dark:text-slate-300 dark:hover:bg-slate-800',
  }[variant]
  return (
    <button
      type="button"
      {...rest}
      disabled={disabled || loading}
      className={cx('inline-flex items-center justify-center gap-2 rounded-lg px-3.5 py-2 text-sm font-medium transition focus:outline-none focus-visible:ring-2 focus-visible:ring-brand-500 disabled:cursor-not-allowed', styles, className)}
    >
      {loading ? <Spinner /> : icon}
      {children}
    </button>
  )
}

interface FieldProps {
  label: string
  hint?: ReactNode
  error?: string
  children: (props: { id: string; 'aria-invalid': boolean; 'aria-describedby': string | undefined }) => ReactNode
}

/** Rótulo + controlo + dica/erro, ligados por ids (acessível e testável por rótulo). */
export function Field({ label, hint, error, children }: FieldProps) {
  const id = useId()
  const descId = error || hint ? `${id}-desc` : undefined
  return (
    <div className="space-y-1.5">
      <label htmlFor={id} className="block text-sm font-medium text-slate-700 dark:text-slate-300">
        {label}
      </label>
      {children({ id, 'aria-invalid': !!error, 'aria-describedby': descId })}
      {error ? (
        <p id={descId} role="alert" className="text-xs text-rose-600 dark:text-rose-400">
          {error}
        </p>
      ) : (
        hint && (
          <p id={descId} className="text-xs text-slate-500 dark:text-slate-400">
            {hint}
          </p>
        )
      )}
    </div>
  )
}

const INPUT =
  'w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm text-slate-900 shadow-sm placeholder:text-slate-400 focus:border-brand-500 focus:outline-none focus:ring-2 focus:ring-brand-500/30 aria-[invalid=true]:border-rose-500 dark:border-slate-700 dark:bg-slate-950 dark:text-slate-100'

export function TextInput(props: InputHTMLAttributes<HTMLInputElement>) {
  return <input {...props} className={cx(INPUT, props.className)} />
}

export function Select(props: SelectHTMLAttributes<HTMLSelectElement>) {
  return <select {...props} className={cx(INPUT, props.className)} />
}

interface ChoiceProps<T extends string> {
  legend: string
  value: T
  onChange: (value: T) => void
  options: { value: T; title: string; description: string; icon?: ReactNode }[]
}

/** Cartões de escolha única (radios acessíveis). */
export function Choice<T extends string>({ legend, value, onChange, options }: ChoiceProps<T>) {
  return (
    <fieldset>
      <legend className="mb-1.5 text-sm font-medium text-slate-700 dark:text-slate-300">{legend}</legend>
      <div className="grid gap-3 sm:grid-cols-2" role="radiogroup" aria-label={legend}>
        {options.map((o) => {
          const active = o.value === value
          return (
            <label
              key={o.value}
              className={cx(
                'flex cursor-pointer gap-3 rounded-xl border p-3.5 transition',
                active
                  ? 'border-brand-500 bg-brand-50 ring-2 ring-brand-500/20 dark:bg-indigo-950/40'
                  : 'border-slate-200 hover:border-slate-300 dark:border-slate-800 dark:hover:border-slate-700',
              )}
            >
              <input type="radio" name={legend} className="sr-only" checked={active} onChange={() => onChange(o.value)} aria-label={o.title} />
              {o.icon && <span className="mt-0.5 text-brand-600 dark:text-indigo-400">{o.icon}</span>}
              <span>
                <span className="block text-sm font-semibold text-slate-900 dark:text-slate-100">{o.title}</span>
                <span className="block text-xs text-slate-500 dark:text-slate-400">{o.description}</span>
              </span>
            </label>
          )
        })}
      </div>
    </fieldset>
  )
}

export function Notice({ tone, children }: { tone: 'info' | 'warn' | 'bad' | 'good'; children: ReactNode }) {
  const styles = {
    info: 'border-sky-200 bg-sky-50 text-sky-900 dark:border-sky-900 dark:bg-sky-950/40 dark:text-sky-200',
    warn: 'border-amber-200 bg-amber-50 text-amber-900 dark:border-amber-900 dark:bg-amber-950/40 dark:text-amber-200',
    bad: 'border-rose-200 bg-rose-50 text-rose-900 dark:border-rose-900 dark:bg-rose-950/40 dark:text-rose-200',
    good: 'border-emerald-200 bg-emerald-50 text-emerald-900 dark:border-emerald-900 dark:bg-emerald-950/40 dark:text-emerald-200',
  }[tone]
  return (
    <div role={tone === 'bad' ? 'alert' : 'status'} className={cx('rounded-xl border px-4 py-3 text-sm', styles)}>
      {children}
    </div>
  )
}

export function Pre({ children, label }: { children: ReactNode; label?: string }) {
  return (
    <pre aria-label={label} className="max-h-96 overflow-auto whitespace-pre-wrap rounded-xl bg-slate-900 p-4 font-mono text-xs leading-relaxed text-slate-100">
      {children}
    </pre>
  )
}
