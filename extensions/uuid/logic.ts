export function uuidv4(): string {
  return crypto.randomUUID()
}

export function nanoId(size = 21): string {
  const chars = 'useandom-26T198340PX75pxJACKVERYMINDBUSHWOLF_GQZbfghjklqvwyzrict'
  const bytes = crypto.getRandomValues(new Uint8Array(size))
  let id = ''
  for (let i = 0; i < size; i++) {
    id += chars[bytes[i] % chars.length]
  }
  return id
}
