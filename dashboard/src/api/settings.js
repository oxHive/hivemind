import { request } from './client.js'

export const getSyncSettings = () => request('GET', '/api/v1/settings/sync')
export const saveSyncSettings = (body) => request('POST', '/api/v1/settings/sync', body)
