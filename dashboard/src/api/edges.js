import { request } from './client.js'

export const listEdges = () => request('GET', '/api/v1/edges')
export const createEdge = (body) => request('POST', '/api/v1/edges', body)
export const patchEdge = (id, body) => request('PATCH', `/api/v1/edges/${id}`, body)
