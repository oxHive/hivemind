import { request } from './client.js'

export const startSession = () => request('POST', '/api/v1/suggest-sessions')
export const getSession = () => request('GET', '/api/v1/suggest-sessions/current')
export const reviseSession = (edgeId, feedback) =>
  request('POST', '/api/v1/suggest-sessions/current/revise', { edge_id: edgeId, feedback })
export const endSession = () => request('DELETE', '/api/v1/suggest-sessions/current')
