export default {
  id: 'uuid',

  async onSearch(query, _ctx) {
    var trimmed = (query || '').trim()
    var count = parseInt(trimmed)
    var items = []

    if (isNaN(count) || count <= 1) {
      var uuid = crypto.randomUUID()
      var nano = generateNanoId()
      items.push(
        { id: 'v4-standard', title: uuid, subtitle: 'UUID v4 (标准)' },
        {
          id: 'v4-nodash',
          title: uuid.replace(/-/g, ''),
          subtitle: 'UUID v4 (无短横线)',
        },
        {
          id: 'v4-upper',
          title: uuid.toUpperCase(),
          subtitle: 'UUID v4 (大写)',
        },
        { id: 'nanoid', title: nano, subtitle: 'NanoID' },
      )
    } else {
      var total = Math.min(count, 100)
      for (var i = 0; i < total; i++) {
        var uuid = crypto.randomUUID()
        items.push({ id: 'multi-' + i, title: uuid, subtitle: 'UUID v4 (' + (i + 1) + ')' })
      }
    }

    return { type: 'list', items: items }
  },

  async onAction(actionId, payload, ctx) {
    if (actionId === 'copy' || actionId === 'execute') {
      var text = payload.item?.title || ''
      if (text && ctx.clipboard) {
        await ctx.clipboard.write(text)
      }
      await ctx.ui.hide()
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
