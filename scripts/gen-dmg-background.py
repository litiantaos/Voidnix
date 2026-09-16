#!/usr/bin/env python3
"""生成 DMG 安装背景（拖拽引导：一个箭头 + 一句话）。

- 逻辑尺寸 660×400pt，与 tauri.conf.json bundle.macOS.dmg.windowSize 同步
- @2x 输出 1320×800px + pHYs 144dpi：Finder 按内嵌 DPI 折算逻辑尺寸，retina 屏全分辨率渲染
- 图标槽位不画进背景（Finder 渲染真实图标）：中心位与 dmg.appPosition / applicationFolderPosition 同步，
  icon size 128 为 tauri-bundler 固定值
- 改动布局须同步 tauri.conf.json 的 dmg 段，反向亦然

bun run generate:dmg-bg
"""

import glob

from PIL import Image, ImageDraw, ImageFilter, ImageFont

SCALE = 2
SS = 4  # 箭头超采样倍率（8x 物理 → LANCZOS 缩回 2x，圆角边缘平滑）
W, H = 660, 400  # pt

# 设计 token（与 src/styles/theme.css 浅色轨对齐）
ACCENT = (61, 130, 240)  # --color-accent #3d82f0
INK = (40, 40, 50)  # --shadow-ink
TEXT_PRIMARY = (30, 35, 50, 0.92)  # --color-text-primary
BG_TOP = (244, 246, 249)
BG_WHITE_Y = 320  # 渐变在此淡出到纯白：下方与窗口白带无缝衔接

# 实测窗口几何（macOS 26，toolbar/statusbar 均 hidden）：660×400pt 窗口 = 32pt 标题栏
# + 340pt 背景绘制区（图标位置即此区坐标）+ 28pt 隐藏状态栏白带。全部内容须落在顶部 340pt 内。
DRAWABLE_H = 340

SENTENCE = "将 Voidnix 拖入 Applications 文件夹"
SENTENCE_CENTER = (330, 88)  # 光学中心
SENTENCE_SIZE = 16  # pt，与 Finder 图标标签字号（tauri-bundler 固定 text size 16）同阶

# 箭头：两图标中心连线高度，图标内缘 244/416，长 120pt 居中，全形状圆角（尾端平头只圆角不圆帽）
ARROW_Y = 210  # = 图标中心 y（dmg.appPosition.y）
ARROW_X0 = 270
ARROW_TIP = 390
ARROW_STROKE = 6
ARROW_HEAD_BASE = 372
ARROW_HEAD_HALF = 10
ARROW_R_TAIL = 2  # 尾端角（< 线宽一半，保留平头）
ARROW_R_JOINT = 2  # 杆身 → 箭头过渡凹角
ARROW_R_BACK = 3  # 箭头后角
ARROW_R_TIP = 2.5  # 顶角

OUT = "src-tauri/icons/dmg-background.png"


def load_font(size_px):
    """PingFang SC Medium：macOS 26 起字体随 OTA 资产目录下发，路径含哈希，glob 兜底。"""
    cands = sorted(glob.glob("/System/Library/AssetsV2/com_apple_MobileAsset_Font8/*/AssetData/PingFang.ttc"))
    cands.append("/System/Library/Fonts/PingFang.ttc")
    for path in cands:
        for i in range(12):
            try:
                probe = ImageFont.truetype(path, 32, index=i)
            except OSError:
                break
            family, style = probe.getname()
            if family == "PingFang SC" and style == "Medium":
                return ImageFont.truetype(path, size_px, index=i)
    raise SystemExit("未找到 PingFang SC Medium")


def gradient():
    img = Image.new("RGB", (W * SCALE, H * SCALE))
    px = img.load()
    white = (255, 255, 255)
    for y in range(H * SCALE):
        t = min(y / SCALE / BG_WHITE_Y, 1.0)
        c = tuple(round(a + (b - a) * t) for a, b in zip(BG_TOP, white))
        for x in range(W * SCALE):
            px[x, y] = c
    return img


def accent_mist():
    """顶部居中极淡 accent 雾（--accent-wash 意向，峰值 3.5%），1/4 尺寸计算后放大。"""
    sw, sh = W // 4, H // 4
    mask = Image.new("L", (sw, sh), 0)
    px = mask.load()
    cx, cy, r = 330 / 4, -100 / 4, 320 / 4
    for y in range(sh):
        for x in range(sw):
            d = ((x - cx) ** 2 + (y - cy) ** 2) ** 0.5 / r
            if d < 1:
                px[x, y] = round(255 * 0.035 * (1 - d) ** 2)
    mask = mask.resize((W * SCALE, H * SCALE), Image.LANCZOS)
    layer = Image.new("RGBA", (W * SCALE, H * SCALE), (*ACCENT, 0))
    layer.putalpha(mask)
    return layer


def rounded_polygon(draw, verts, fill, steps=10):
    """圆角多边形：每个顶点以二次贝塞尔（顶点为控制点）圆角化，凸凹角通用。verts 为 ((x, y), r) 列表。"""
    n = len(verts)
    out = []
    for i, (p, r) in enumerate(verts):
        prev = verts[i - 1][0]
        nxt = verts[(i + 1) % n][0]
        v1 = (p[0] - prev[0], p[1] - prev[1])
        v2 = (nxt[0] - p[0], nxt[1] - p[1])
        d1 = (v1[0] ** 2 + v1[1] ** 2) ** 0.5
        d2 = (v2[0] ** 2 + v2[1] ** 2) ** 0.5
        r = min(r, d1 / 2, d2 / 2)
        pin = (p[0] - v1[0] / d1 * r, p[1] - v1[1] / d1 * r)
        pout = (p[0] + v2[0] / d2 * r, p[1] + v2[1] / d2 * r)
        out.append(pin)
        for t in range(1, steps):
            k = t / steps
            out.append(
                (
                    (1 - k) ** 2 * pin[0] + 2 * (1 - k) * k * p[0] + k**2 * pout[0],
                    (1 - k) ** 2 * pin[1] + 2 * (1 - k) * k * p[1] + k**2 * pout[1],
                )
            )
        out.append(pout)
    draw.polygon(out, fill=fill)


def arrow_layer(fill):
    """超采样画布上绘制圆角箭头，缩回主分辨率返回 RGBA 图层。"""
    s = SCALE * SS
    layer = Image.new("RGBA", (W * s, H * s), (0, 0, 0, 0))
    draw = ImageDraw.Draw(layer)
    y = ARROW_Y * s
    hw, hh = ARROW_STROKE / 2 * s, ARROW_HEAD_HALF * s
    x0, xb, tip = ARROW_X0 * s, ARROW_HEAD_BASE * s, ARROW_TIP * s
    verts = [
        ((x0, y - hw), ARROW_R_TAIL * s),
        ((xb, y - hw), ARROW_R_JOINT * s),
        ((xb, y - hh), ARROW_R_BACK * s),
        ((tip, y), ARROW_R_TIP * s),
        ((xb, y + hh), ARROW_R_BACK * s),
        ((xb, y + hw), ARROW_R_JOINT * s),
        ((x0, y + hw), ARROW_R_TAIL * s),
    ]
    rounded_polygon(draw, verts, fill)
    return layer.resize((W * SCALE, H * SCALE), Image.LANCZOS)


def main():
    assert ARROW_Y + 64 + 22 < DRAWABLE_H, "图标 + 标签超出背景绘制区（340pt）"
    img = gradient().convert("RGBA")
    img.alpha_composite(accent_mist())

    # 箭头投影（--shadow-bar 量级的极淡墨影）
    shadow = arrow_layer((*INK, 26)).filter(ImageFilter.GaussianBlur(3 * SCALE))
    img.alpha_composite(shadow, (0, int(1.5 * SCALE)))
    img.alpha_composite(arrow_layer((*ACCENT, 255)))

    # 文字色：text-primary 半透明混入所在高度的背景色
    t = min(SENTENCE_CENTER[1] / BG_WHITE_Y, 1.0)
    bg_at = tuple(a + ((255 - a) * t) for a in BG_TOP)
    a = TEXT_PRIMARY[3]
    color = tuple(round(TEXT_PRIMARY[i] * a + bg_at[i] * (1 - a)) for i in range(3))
    font = load_font(SENTENCE_SIZE * SCALE)
    draw = ImageDraw.Draw(img)
    draw.text(
        (SENTENCE_CENTER[0] * SCALE, SENTENCE_CENTER[1] * SCALE),
        SENTENCE,
        font=font,
        fill=color,
        anchor="mm",
    )

    img.convert("RGB").save(OUT, dpi=(144, 144))
    bbox = draw.textbbox((0, 0), SENTENCE, font=font)
    print(f"{OUT}: {W * SCALE}x{H * SCALE}px @{144}dpi (逻辑 {W}x{H}pt), 句宽 {(bbox[2] - bbox[0]) / SCALE:.0f}pt")


if __name__ == "__main__":
    main()
