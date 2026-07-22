import { request } from './client.js'

export const getSyncSettings = () => request('GET', '/api/v1/settings/sync')
export const saveSyncSettings = (body) => request('POST', '/api/v1/settings/sync', body)
export const getTagSettings = () => request('GET', '/api/v1/settings/tags')
export const saveTagSettings = (body) => request('POST', '/api/v1/settings/tags', body)
export const getContentLimitSettings = () => request('GET', '/api/v1/settings/content-limits')
export const saveContentLimitSettings = (body) => request('POST', '/api/v1/settings/content-limits', body)
