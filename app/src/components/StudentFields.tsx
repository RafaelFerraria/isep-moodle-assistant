import { useMemo, useState } from 'react'
import type { Institution } from '../api'
import { useApp } from '../lib/context'
import { useAsync } from '../lib/hooks'
import { Field, Select, Spinner, TextInput } from './ui'

export interface StudentValue {
  institutionId: string
  institutionName: string
  course: string
  year: number
}

export const EMPTY_STUDENT: StudentValue = { institutionId: '', institutionName: '', course: '', year: 0 }

const YEARS = [1, 2, 3, 4, 5, 6]

/** Mesmas regras do servidor, para avisar antes de enviar. */
export function validateStudent(v: StudentValue): Record<string, string> {
  const errors: Record<string, string> = {}
  if (!v.institutionId) errors['student.institution_id'] = 'Escolhe a tua instituição.'
  if (v.institutionId === 'outra' && v.institutionName.trim().length < 2) errors['student.institution_name'] = 'Indica o nome da instituição.'
  if (v.course.trim().length < 2) errors['student.course'] = 'Indica o teu curso.'
  if (!v.year) errors['student.year'] = 'Escolhe o ano.'
  return errors
}

/** Instituição (com pesquisa), curso e ano. Fica guardado só neste PC; mais tarde irá para a conta online. */
export function StudentFields({ value, onChange, errors = {} }: { value: StudentValue; onChange: (v: StudentValue) => void; errors?: Record<string, string> }) {
  const { api } = useApp()
  const list = useAsync(() => api.institutions(), [api])
  const [query, setQuery] = useState('')

  const options: Institution[] = useMemo(() => {
    const q = query.trim().toLowerCase()
    const items = list.data?.items ?? []
    const shown = q ? items.filter((i) => i.name.toLowerCase().includes(q) || i.id.includes(q)) : items
    // a escolhida nunca desaparece da lista, mesmo que a pesquisa a esconda
    const chosen = items.find((i) => i.id === value.institutionId)
    return chosen && !shown.includes(chosen) ? [chosen, ...shown] : shown
  }, [list.data, query, value.institutionId])

  if (list.loading && !list.data) {
    return (
      <div className="flex items-center gap-2 text-sm text-slate-500" role="status">
        <Spinner /> A carregar as instituições…
      </div>
    )
  }

  return (
    <div className="space-y-4">
      <Field label="Pesquisar instituição" hint="Escreve parte do nome para filtrar a lista.">
        {(p) => <TextInput {...p} value={query} onChange={(e) => setQuery(e.target.value)} placeholder="ex.: Porto, Minho, Politécnico…" />}
      </Field>
      <Field label="Instituição" error={errors['student.institution_id']}>
        {(p) => (
          <Select {...p} value={value.institutionId} onChange={(e) => onChange({ ...value, institutionId: e.target.value })}>
            <option value="">Escolhe…</option>
            {options.map((i) => (
              <option key={i.id} value={i.id}>
                {i.name}
              </option>
            ))}
            <option value="outra">Outra instituição…</option>
          </Select>
        )}
      </Field>
      {value.institutionId === 'outra' && (
        <Field label="Nome da instituição" error={errors['student.institution_name']}>
          {(p) => <TextInput {...p} value={value.institutionName} onChange={(e) => onChange({ ...value, institutionName: e.target.value })} />}
        </Field>
      )}
      <div className="grid gap-4 sm:grid-cols-[1fr_10rem]">
        <Field label="Curso" error={errors['student.course']}>
          {(p) => <TextInput {...p} value={value.course} onChange={(e) => onChange({ ...value, course: e.target.value })} placeholder="ex.: Licenciatura em Engenharia Informática" />}
        </Field>
        <Field label="Ano" error={errors['student.year']}>
          {(p) => (
            <Select {...p} value={value.year || ''} onChange={(e) => onChange({ ...value, year: Number(e.target.value) })}>
              <option value="">Escolhe…</option>
              {YEARS.map((y) => (
                <option key={y} value={y}>
                  {y}º ano
                </option>
              ))}
            </Select>
          )}
        </Field>
      </div>
    </div>
  )
}
