import { describe, expect, it } from 'vitest'
import { fmtBytes, fmtDateTime, fmtDuration, fmtRelative, parseLocal, shortCourse } from './format'

const NOW = new Date(2026, 9, 26, 19, 40) // seg 26/10/2026 19:40

describe('formatação', () => {
  it('lê datas locais do servidor', () => {
    expect(parseLocal('2026-10-26T18:00:00')).toEqual(new Date(2026, 9, 26, 18, 0, 0))
    expect(parseLocal('2026-10-26')).toEqual(new Date(2026, 9, 26))
    expect(parseLocal(null)).toBeNull()
    expect(parseLocal('lixo')).toBeNull()
  })

  it('formata dia da semana, data e hora', () => {
    expect(fmtDateTime('2026-10-26T18:05:00')).toBe('seg 26/10 18:05')
    expect(fmtDateTime(undefined)).toBe('—')
  })

  it('descreve o tempo relativo', () => {
    expect(fmtRelative('2026-10-26T23:59:00', NOW)).toBe('hoje às 23:59')
    expect(fmtRelative('2026-10-27T18:00:00', NOW)).toBe('amanhã às 18:00')
    expect(fmtRelative('2026-10-30T18:00:00', NOW)).toBe('sex 30/10 18:00')
    expect(fmtRelative('2026-10-26T19:39:30', NOW)).toBe('agora mesmo')
    expect(fmtRelative('2026-10-26T19:10:00', NOW)).toBe('há 30 min')
    expect(fmtRelative('2026-10-26T16:00:00', NOW)).toBe('há 3 h')
    expect(fmtRelative('2026-10-25T21:00:00', NOW)).toBe('ontem às 21:00')
    expect(fmtRelative('2026-10-20T10:00:00', NOW)).toBe('ter 20/10 10:00')
  })

  it('formata durações e tamanhos', () => {
    expect(fmtDuration(4.26)).toBe('4.3 s')
    expect(fmtDuration(41.2)).toBe('41 s')
    expect(fmtDuration(125)).toBe('2 min 5 s')
    expect(fmtDuration(null)).toBe('—')
    expect(fmtBytes(500)).toBe('500 B')
    expect(fmtBytes(482000)).toBe('471 KB')
    expect(fmtBytes(5 * 1024 * 1024)).toBe('5.0 MB')
  })

  it('encurta o nome da disciplina', () => {
    expect(shortCourse('DEI - Laboratório / Projeto III - 1º Semestre 2026/2027')).toBe('Laboratório / Projeto III')
    expect(shortCourse('Gabinete de Relações Externas')).toBe('Gabinete de Relações Externas')
  })
})
