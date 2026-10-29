import { useCallback, useEffect, useRef, useState } from 'react'
import { ApiError } from '../api'
import type { Job } from '../api'
import { useApp } from './context'

export interface AsyncState<T> {
  data: T | null
  error: ApiError | null
  loading: boolean
  reload: () => Promise<void>
}

/** Carrega dados ao montar (e quando `deps` mudam), opcionalmente de `intervalMs` em `intervalMs`. */
export function useAsync<T>(fn: () => Promise<T>, deps: unknown[], intervalMs = 0): AsyncState<T> {
  const [data, setData] = useState<T | null>(null)
  const [error, setError] = useState<ApiError | null>(null)
  const [loading, setLoading] = useState(true)
  const fnRef = useRef(fn)
  fnRef.current = fn
  const alive = useRef(true)

  const reload = useCallback(async () => {
    try {
      const value = await fnRef.current()
      if (!alive.current) return
      setData(value)
      setError(null)
    } catch (e) {
      if (!alive.current) return
      setError(e instanceof ApiError ? e : new ApiError(0, String(e)))
    } finally {
      if (alive.current) setLoading(false)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps)

  useEffect(() => {
    alive.current = true
    setLoading(true)
    void reload()
    const timer = intervalMs > 0 ? setInterval(() => void reload(), intervalMs) : null
    return () => {
      alive.current = false
      if (timer) clearInterval(timer)
    }
  }, [reload, intervalMs])

  return { data, error, loading, reload }
}

export type JobKind = 'digest_preview' | 'digest' | 'moodle_sync' | 'session_pair' | 'session_test'

export interface JobRunner {
  job: Job | null
  running: boolean
  error: ApiError | null
  start: (kind: JobKind) => Promise<void>
  dismiss: () => void
}

/** Arranca uma tarefa em segundo plano e consulta o progresso até terminar. Trata o 409 (já há uma a correr). */
export function useJobRunner(onFinished?: (job: Job) => void): JobRunner {
  const { api, pollMs } = useApp()
  const [job, setJob] = useState<Job | null>(null)
  const [error, setError] = useState<ApiError | null>(null)
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const alive = useRef(true)
  const finished = useRef(onFinished)
  finished.current = onFinished

  useEffect(() => {
    alive.current = true
    return () => {
      alive.current = false
      if (timer.current) clearTimeout(timer.current)
    }
  }, [])

  const poll = useCallback(
    (id: string, seen: string[]) => {
      timer.current = setTimeout(async () => {
        try {
          const next = await api.job(id, seen.length)
          if (!alive.current) return
          const progress = [...seen, ...next.progress]
          const merged = { ...next, progress }
          setJob(merged)
          if (next.status === 'running') poll(id, progress)
          else finished.current?.(merged)
        } catch (e) {
          if (!alive.current) return
          setError(e instanceof ApiError ? e : new ApiError(0, String(e)))
        }
      }, pollMs)
    },
    [api, pollMs],
  )

  const start = useCallback(
    async (kind: JobKind) => {
      setError(null)
      setJob(null)
      try {
        const starters: Record<JobKind, () => Promise<Job>> = {
          digest_preview: () => api.startDigestPreview(),
          digest: () => api.startDigestRun(),
          moodle_sync: () => api.startMoodleSync(),
          session_pair: () => api.startSessionPair(),
          session_test: () => api.startSessionTest(),
        }
        const started = await starters[kind]()
        if (!alive.current) return
        setJob(started)
        poll(started.id, started.progress)
      } catch (e) {
        if (!alive.current) return
        const err = e instanceof ApiError ? e : new ApiError(0, String(e))
        if (err.status === 409 && err.activeJobId) {
          // outra tarefa já estava a correr: acompanha-a em vez de falhar
          const active = await api.job(err.activeJobId)
          setJob(active)
          poll(active.id, active.progress)
        } else setError(err)
      }
    },
    [api, poll],
  )

  const dismiss = useCallback(() => {
    if (timer.current) clearTimeout(timer.current)
    setJob(null)
    setError(null)
  }, [])

  return { job, running: job?.status === 'running', error, start, dismiss }
}
