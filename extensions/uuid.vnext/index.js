export default {
  id: 'uuid',

  async onSearch(_query, _ctx) {
    var uuid = crypto.randomUUID()
    var nano = generateNanoId()
    return {
      type: 'list',
      items: [
        { id: 'v4-standard', title: uuid, subtitle: 'UUID v4', icon: 'i-ri-key-2-line' },
        {
          id: 'v4-nodash',
          title: uuid.replace(/-/g, ''),
          subtitle: 'UUID v4（无短横线）',
          icon: 'i-ri-key-2-line',
        },
        {
          id: 'v4-upper',
          title: uuid.toUpperCase(),
          subtitle: 'UUID v4（大写）',
          icon: 'i-ri-key-2-line',
        },
        { id: 'nanoid', title: nano, subtitle: 'NanoID', icon: 'i-ri-key-2-line' },
      ],
    }
  },
}

function generateNanoId(size) {
  size = size || 21
  var urlAlphabet = 'useandom-26T198340PX75pxJACKVERYMINDBUSHWOLF_GQZbfghjklqvwyzrict'
  var id = ''
  var i = size
  while (i > 0) {
    i -= 1
    id += urlAlphabet[(Math.random() * 64) | 0]
  }
  return id
}
