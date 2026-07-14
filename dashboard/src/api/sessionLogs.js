import { request } from './client.js'

export const getSessionLogs = (limit = 50) => request('GET', `/api/v1/session-logs?limit=${limit}`)
