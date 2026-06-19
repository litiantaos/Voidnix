import type { SearchResult } from '@/runtime/types'

export function getFileIcon(result: SearchResult): { icon: string; color: string } {
  if (result.data?.kind === 'folder') {
    const name = result.title.toLowerCase()
    const path = ((result.data?.path as string) || '').toLowerCase()
    if (name.endsWith('.app') || path.includes('.app/')) {
      return { icon: 'i-ri-apps-2-fill', color: 'text-blue-500' }
    }
    if (name.startsWith('.git') || path.includes('/.git/')) {
      return { icon: 'i-ri-git-branch-fill', color: 'text-orange-500' }
    }
    if (name.endsWith('.xcodeproj') || name.endsWith('.xcworkspace')) {
      return { icon: 'i-ri-code-s-slash-fill', color: 'text-orange-500' }
    }
    return { icon: 'i-ri-folder-fill', color: 'text-blue-400' }
  }

  const ext = ((result.data?.path as string) || '').split('.').pop()?.toLowerCase()
  switch (ext) {
    case 'png':
    case 'jpg':
    case 'jpeg':
    case 'gif':
    case 'svg':
    case 'webp':
    case 'ico':
    case 'bmp':
    case 'heic':
    case 'tiff':
      return { icon: 'i-ri-image-fill', color: 'text-purple-400' }
    case 'js':
    case 'ts':
    case 'jsx':
    case 'tsx':
    case 'vue':
    case 'json':
    case 'rs':
    case 'py':
    case 'html':
    case 'css':
    case 'scss':
    case 'less':
    case 'cpp':
    case 'c':
    case 'h':
    case 'go':
    case 'java':
    case 'kt':
    case 'swift':
    case 'rb':
    case 'php':
    case 'sql':
    case 'yaml':
    case 'yml':
    case 'toml':
    case 'xml':
    case 'dart':
      return { icon: 'i-ri-code-fill', color: 'text-yellow-500' }
    case 'txt':
    case 'md':
    case 'mdx':
    case 'csv':
    case 'log':
      return { icon: 'i-ri-file-text-fill', color: 'text-black/40' }
    case 'pdf':
      return { icon: 'i-ri-file-pdf-fill', color: 'text-red-500' }
    case 'doc':
    case 'docx':
    case 'pages':
      return { icon: 'i-ri-file-word-fill', color: 'text-blue-500' }
    case 'xls':
    case 'xlsx':
    case 'numbers':
      return { icon: 'i-ri-file-excel-fill', color: 'text-green-500' }
    case 'ppt':
    case 'pptx':
    case 'key':
      return { icon: 'i-ri-file-ppt-fill', color: 'text-orange-500' }
    case 'zip':
    case 'rar':
    case '7z':
    case 'tar':
    case 'gz':
    case 'bz2':
    case 'xz':
      return { icon: 'i-ri-file-zip-fill', color: 'text-red-400' }
    case 'mp3':
    case 'wav':
    case 'flac':
    case 'm4a':
    case 'aac':
    case 'ogg':
      return { icon: 'i-ri-music-fill', color: 'text-pink-400' }
    case 'mp4':
    case 'mkv':
    case 'avi':
    case 'mov':
    case 'webm':
      return { icon: 'i-ri-video-fill', color: 'text-indigo-400' }
    case 'sh':
    case 'bash':
    case 'zsh':
    case 'fish':
      return { icon: 'i-ri-terminal-box-fill', color: 'text-green-500' }
    case 'dmg':
    case 'pkg':
    case 'exe':
    case 'msi':
    case 'apk':
      return { icon: 'i-ri-box-3-fill', color: 'text-orange-400' }
    case 'psd':
    case 'ai':
    case 'sketch':
    case 'fig':
    case 'xd':
      return { icon: 'i-ri-brush-fill', color: 'text-pink-500' }
    case 'ttf':
    case 'otf':
    case 'woff':
    case 'woff2':
      return { icon: 'i-ri-font-size-2', color: 'text-teal-500' }
    default:
      return { icon: 'i-ri-file-fill', color: 'text-black/40' }
  }
}
