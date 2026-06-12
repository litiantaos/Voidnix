export default {
  id: 'base64',

  async onSearch(query, _ctx) {
    const trimmed = (query || '').trim()
    if (!trimmed) {
      return { type: 'list', items: [], emptyText: '输入文本进行 Base64 编解码' }
    }

    const items = []

    const encoded = encodeBase64(trimmed)
    if (encoded) {
      items.push({
        id: 'encoded',
        title: encoded,
        subtitle: 'Base64 编码结果',
        icon: 'i-ri-code-s-slash-line',
      })
    }

    if (/^[A-Za-z0-9+/=]+$/.test(trimmed) && trimmed.length % 4 === 0) {
      const decoded = decodeBase64(trimmed)
      if (decoded) {
        items.push({
          id: 'decoded',
          title: decoded,
          subtitle: 'Base64 解码结果',
          icon: 'i-ri-text',
        })
      }
    }

    return { type: 'list', items }
  },
}

function encodeBase64(str) {
  try {
    return btoa(
      encodeURIComponent(str).replace(/%([0-9A-F]{2})/g, function (_, p1) {
        return String.fromCharCode(Number('0x' + p1))
      }),
    )
  } catch {
    return ''
  }
}

function decodeBase64(str) {
  try {
    return decodeURIComponent(
      atob(str)
        .split('')
        .map(function (c) {
          return '%' + ('00' + c.charCodeAt(0).toString(16)).slice(-2)
        })
        .join(''),
    )
  } catch {
    return ''
  }
}
