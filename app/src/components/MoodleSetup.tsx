import { FolderOpen, RotateCcw } from 'lucide-react'
import { useEffect, useState } from 'react'
import { ApiError } from '../api'
import type { Settings } from '../api'
import { useApp } from '../lib/context'
import { Badge, Button, Card, Field, Notice, TextInput } from './ui'

const SOURCE_LABEL: Record<Settings['moodle']['url_source'], string> = {
  institution: 'Detetado pela tua instituição',
  custom: 'Indicado por ti',
  default: 'Predefinido (ISEP)',
  missing: 'Em falta',
}

/**
 * Moodle: endereço (detetado pela instituição ou pedido ao utilizador e validado) e pasta de downloads, com a pasta
 * predefinida à vista e a opção de a mudar ou desativar logo aqui. Cada alteração é guardada de imediato.
 */
export function MoodleSetup({ moodle, onChanged }: { moodle: Settings['moodle']; onChanged: () => void }) {
  const { api } = useApp()
  const [url, setUrl] = useState(moodle.url ?? '')
  const [editingUrl, setEditingUrl] = useState(false)
  const [dir, setDir] = useState(moodle.download_dir)
  const [editingDir, setEditingDir] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [urlOk, setUrlOk] = useState<string | null>(null)
  useEffect(() => setUrl(moodle.url ?? ''), [moodle.url])
  useEffect(() => setDir(moodle.download_dir), [moodle.download_dir])

  const run = async (work: () => Promise<void>) => {
    setBusy(true)
    setError(null)
    try {
      await work()
      onChanged()
    } catch (e) {
      setError(e instanceof ApiError ? (Object.values(e.fields)[0] ?? e.message) : String(e))
    } finally {
      setBusy(false)
    }
  }

  const saveUrl = () =>
    run(async () => {
      const checked = await api.probeMoodle(url)
      await api.updateSettings({ moodle: { url: checked.url } })
      setUrlOk(`Moodle confirmado: ${checked.url}`)
      setEditingUrl(false)
    })

  const saveDir = (path: string) =>
    run(async () => {
      await api.updateSettings({ moodle: { download_dir: path } })
      setEditingDir(false)
    })

  const pick = async () => {
    const chosen = await window.isepBridge?.pickFolder?.()
    if (chosen) await saveDir(chosen)
    else if (!window.isepBridge?.pickFolder) setEditingDir(true)
  }

  const missing = moodle.url_source === 'missing'

  return (
    <Card title="Moodle" subtitle="Onde a app vai buscar os ficheiros das tuas disciplinas.">
      <div className="space-y-6">
        <section aria-label="Ligação ao Moodle" className="space-y-3">
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="text-sm font-semibold text-slate-900 dark:text-slate-100">Endereço</h3>
            <Badge tone={missing ? 'warn' : 'good'}>{SOURCE_LABEL[moodle.url_source]}</Badge>
          </div>
          {missing || editingUrl ? (
            <div className="space-y-3">
              {missing && <Notice tone="info">Não conheço o Moodle da tua instituição. Cola o endereço que usas no browser; eu confirmo que é um Moodle.</Notice>}
              <div className="flex flex-wrap items-end gap-3">
                <div className="min-w-64 flex-1">
                  <Field label="Endereço do Moodle" hint="Ex.: https://moodle.escola.pt">
                    {(p) => <TextInput {...p} value={url} onChange={(e) => setUrl(e.target.value)} placeholder="https://moodle.escola.pt" spellCheck={false} />}
                  </Field>
                </div>
                <Button variant="primary" onClick={() => void saveUrl()} loading={busy} disabled={!url.trim()}>
                  Verificar e guardar
                </Button>
                {editingUrl && !missing && (
                  <Button variant="ghost" onClick={() => { setEditingUrl(false); setUrl(moodle.url ?? '') }}>
                    Cancelar
                  </Button>
                )}
              </div>
            </div>
          ) : (
            <div className="flex flex-wrap items-center gap-3">
              <code className="rounded-lg bg-slate-100 px-2.5 py-1.5 font-mono text-xs dark:bg-slate-800">{moodle.url}</code>
              <Button variant="ghost" onClick={() => setEditingUrl(true)}>
                Alterar
              </Button>
            </div>
          )}
          {urlOk && <p role="status" className="text-xs text-emerald-700 dark:text-emerald-400">{urlOk}</p>}
        </section>

        <section aria-label="Descarregamento" className="space-y-3 border-t border-slate-100 pt-5 dark:border-slate-800">
          <h3 className="text-sm font-semibold text-slate-900 dark:text-slate-100">Ficheiros descarregados</h3>
          <label className="flex cursor-pointer items-start gap-3">
            <input
              type="checkbox"
              checked={moodle.auto_download}
              disabled={busy}
              onChange={(e) => void run(async () => void (await api.updateSettings({ moodle: { auto_download: e.target.checked } })))}
              className="mt-1 size-4 rounded border-slate-300 text-brand-600 focus:ring-brand-500"
            />
            <span>
              <span className="block text-sm font-medium text-slate-900 dark:text-slate-100">Descarregar automaticamente</span>
              <span className="block text-xs text-slate-500 dark:text-slate-400">
                Antes de cada resumo a app descarrega os ficheiros novos. Desligado, só descarrega quando carregares em «Sincronizar».
              </span>
            </span>
          </label>

          <div className="space-y-2">
            <p className="text-xs font-medium text-slate-500">Pasta de destino</p>
            {editingDir ? (
              <div className="flex flex-wrap items-end gap-3">
                <div className="min-w-64 flex-1">
                  <Field label="Pasta de destino">{(p) => <TextInput {...p} value={dir} onChange={(e) => setDir(e.target.value)} spellCheck={false} />}</Field>
                </div>
                <Button variant="primary" onClick={() => void saveDir(dir)} loading={busy} disabled={!dir.trim()}>
                  Guardar pasta
                </Button>
                <Button variant="ghost" onClick={() => { setEditingDir(false); setDir(moodle.download_dir) }}>
                  Cancelar
                </Button>
              </div>
            ) : (
              <div className="flex flex-wrap items-center gap-3">
                <code aria-label="Pasta de destino" className="break-all rounded-lg bg-slate-100 px-2.5 py-1.5 font-mono text-xs dark:bg-slate-800">
                  {moodle.download_dir}
                </code>
                {moodle.download_dir_is_default && <Badge>Predefinida</Badge>}
                <Button variant="secondary" onClick={() => void pick()} disabled={busy} icon={<FolderOpen className="size-4" />}>
                  Alterar pasta…
                </Button>
                {!moodle.download_dir_is_default && (
                  <Button variant="ghost" onClick={() => void saveDir('')} disabled={busy} icon={<RotateCcw className="size-4" />}>
                    Repor a predefinida
                  </Button>
                )}
              </div>
            )}
          </div>
        </section>
        {error && <Notice tone="bad">{error}</Notice>}
      </div>
    </Card>
  )
}
