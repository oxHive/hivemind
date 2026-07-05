import { request } from './client.js'

export const listConflicts = (status = 'pending') =>
  request('GET', `/api/v1/conflicts?status=${status}`)
export const resolveConflict = (id, resolution) =>
  request('POST', `/api/v1/conflicts/${id}/resolve`, { resolution })
export const listFeedback = (status = 'pending') =>
  request('GET', `/api/v1/feedback?status=${status}`)
export const patchFeedback = (id, body) =>
  request('PATCH', `/api/v1/feedback/${id}`, body)
export const createFeedback = (body) => request('POST', '/api/v1/feedback', body)
