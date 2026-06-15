import { request } from './client.js'

export const listConflicts = (status = 'open') =>
  request('GET', `/api/v1/conflicts?status=${status}`)
export const resolveConflict = (id, action) =>
  request('POST', `/api/v1/conflicts/${id}/resolve`, { action })
export const listFeedback = (status = 'open') =>
  request('GET', `/api/v1/feedback?status=${status}`)
export const patchFeedback = (id, body) =>
  request('PATCH', `/api/v1/feedback/${id}`, body)
