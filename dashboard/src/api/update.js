import { request } from './client.js'

export const getUpdateState = () => request('GET', '/api/v1/update')
export const applyUpdate = () => request('POST', '/api/v1/update/apply')
