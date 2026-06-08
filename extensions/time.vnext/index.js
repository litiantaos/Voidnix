export default {
  id: 'time',

  async onSearch(query, _ctx) {
    var trimmed = (query || '').trim()
    var items = []

    if (!trimmed) {
      var now = new Date()
      items.push(
        {
          id: 'local',
          title: formatLocal(now),
          subtitle: '当前时间 (Local)',
          icon: 'i-ri-time-line',
        },
        {
          id: 'ms',
          title: now.getTime().toString(),
          subtitle: '时间戳 (毫秒)',
          icon: 'i-ri-timer-line',
        },
        {
          id: 's',
          title: Math.floor(now.getTime() / 1000).toString(),
          subtitle: '时间戳 (秒)',
          icon: 'i-ri-timer-line',
        },
      )
      return { type: 'list', items: items }
    }

    if (/^\d+$/.test(trimmed)) {
      var num = parseInt(trimmed, 10)
      var dateMs = num > 9999999999 ? new Date(num) : new Date(num * 1000)
      if (!isNaN(dateMs.getTime())) {
        items.push({
          id: 'parsed-time',
          title: formatLocal(dateMs),
          subtitle: '由时间戳 ' + trimmed + ' 解析',
          icon: 'i-ri-calendar-line',
        })
      }
    }

    var parsedDate = new Date(trimmed)
    if (!isNaN(parsedDate.getTime())) {
      items.push(
        {
          id: 'parsed-ms',
          title: parsedDate.getTime().toString(),
          subtitle: '由日期解析 (毫秒)',
          icon: 'i-ri-timer-line',
        },
        {
          id: 'parsed-s',
          title: Math.floor(parsedDate.getTime() / 1000).toString(),
          subtitle: '由日期解析 (秒)',
          icon: 'i-ri-timer-line',
        },
      )
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

function formatLocal(date) {
  return date.toLocaleString('zh-CN', { hour12: false })
}
