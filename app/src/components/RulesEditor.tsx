import type { SummaryRules } from '../api'
import { Field } from './ui'

export const MAX_ENTRIES = 30
export const MIN_LEN = 2
export const MAX_LEN = 60

export type RulesText = Record<keyof SummaryRules, string>

export function rulesToText(r: SummaryRules): RulesText {
  return {
    important_keywords: r.important_keywords.join('\n'),
    ignore_senders: r.ignore_senders.join('\n'),
    ignore_subjects: r.ignore_subjects.join('\n'),
  }
}

/** Uma entrada por linha; limpa espaços, ignora linhas em branco e repetidas (sem distinguir maiúsculas). */
export function textToList(text: string): string[] {
  const out: string[] = []
  for (const line of text.split('\n')) {
    const clean = line.split(/\s+/).filter(Boolean).join(' ')
    if (clean && !out.some((x) => x.toLowerCase() === clean.toLowerCase())) out.push(clean)
  }
  return out
}

/** As mesmas regras que o servidor aplica, para avisar enquanto o utilizador escreve. */
export function validateRules(text: RulesText): Record<string, string> {
  const errors: Record<string, string> = {}
  for (const key of Object.keys(text) as (keyof SummaryRules)[]) {
    const items = textToList(text[key])
    const short = items.find((t) => [...t].length < MIN_LEN)
    const long = items.find((t) => [...t].length > MAX_LEN)
    if (short) errors[key] = `«${short}» é demasiado curto (mínimo ${MIN_LEN} caracteres).`
    else if (long) errors[key] = `«${[...long].slice(0, 20).join('')}…» é demasiado longo (máximo ${MAX_LEN} caracteres).`
    else if (items.length > MAX_ENTRIES) errors[key] = `No máximo ${MAX_ENTRIES} entradas (tens ${items.length}).`
  }
  return errors
}

const FIELDS: { key: keyof SummaryRules; label: string; hint: string; placeholder: string }[] = [
  { key: 'important_keywords', label: 'Palavras importantes', hint: 'Os emails com estas palavras aparecem primeiro, em «Importante». Uma por linha.', placeholder: 'teste\nentrega\nexame' },
  { key: 'ignore_senders', label: 'Remetentes a ignorar', hint: 'Ignora emails cujo remetente contém isto. Uma por linha.', placeholder: 'marketing@escola.pt' },
  { key: 'ignore_subjects', label: 'Assuntos a ignorar', hint: 'Ignora emails cujo assunto contém isto. Uma por linha.', placeholder: 'newsletter' },
]

/** Editor das regras do resumo. Valida ao escrever; `serverErrors` mostra o que o servidor recusou. */
export function RulesEditor({ value, onChange, serverErrors = {} }: { value: RulesText; onChange: (v: RulesText) => void; serverErrors?: Record<string, string> }) {
  const local = validateRules(value)
  return (
    <div className="space-y-4 border-t border-slate-100 pt-5 dark:border-slate-800">
      <div>
        <h3 className="text-sm font-semibold text-slate-900 dark:text-slate-100">Regras do resumo</h3>
        <p className="text-xs text-slate-500 dark:text-slate-400">Opcional. Ajusta o que o resumo mostra; não distingue maiúsculas de minúsculas.</p>
      </div>
      <div className="grid gap-4 lg:grid-cols-3">
        {FIELDS.map((f) => (
          <Field key={f.key} label={f.label} hint={f.hint} error={local[f.key] ?? serverErrors[`summary.rules.${f.key}`]}>
            {(p) => (
              <textarea
                {...p}
                rows={5}
                value={value[f.key]}
                onChange={(e) => onChange({ ...value, [f.key]: e.target.value })}
                placeholder={f.placeholder}
                spellCheck={false}
                className="w-full rounded-lg border border-slate-300 bg-white px-3 py-2 font-mono text-sm text-slate-900 shadow-sm placeholder:text-slate-400 focus:border-brand-500 focus:outline-none focus:ring-2 focus:ring-brand-500/30 aria-[invalid=true]:border-rose-500 dark:border-slate-700 dark:bg-slate-950 dark:text-slate-100"
              />
            )}
          </Field>
        ))}
      </div>
    </div>
  )
}
