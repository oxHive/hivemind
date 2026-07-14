import { request } from './client.js'

export const listMemories = (limit) => request('GET', '/api/v1/memories' + (limit ? `?limit=${limit}` : ''))
export const createMemory = (body) => request('POST', '/api/v1/memories', body)
export const patchMemory = (id, body) => request('PATCH', `/api/v1/memories/${id}`, body)
export const deleteMemory = (id) => request('DELETE', `/api/v1/memories/${id}`)
export const deleteAllMemories = () => request('DELETE', '/api/v1/memories/all')
export const exportMemories = () => request('GET', '/api/v1/export')
export const importMemories = (body) => request('POST', '/api/v1/import', body)
export const getStatus = () => request('GET', '/api/v1/status')
