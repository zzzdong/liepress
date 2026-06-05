"""liepress 测试用图片生成脚本"""
from PIL import Image, ImageDraw, ImageFont
import os

W, H = 400, 200
img = Image.new("RGB", (W, H), "#f5f5f5")
draw = ImageDraw.Draw(img)

# 渐变色条
for x in range(W):
    r = int(50 + 150 * (x / W))
    g = int(50 + 100 * (1 - x / W))
    b = int(200 - 100 * (x / W))
    draw.line((x, 0, x, H // 2), fill=(r, g, b))

# 几何图形
draw.rectangle((50, 110, 150, 180), fill="#4a90d9", outline="#2c5f8a", width=2)
draw.ellipse((180, 110, 280, 180), fill="#e74c3c", outline="#a93226", width=2)
draw.polygon([(320, 180), (360, 110), (360, 180)], fill="#27ae60", outline="#1e8449", width=2)

# 文字
try:
    font = ImageFont.truetype("arial.ttf", 18)
except:
    font = ImageFont.load_default()
draw.text((200, 185), "liepress test image", fill="#333", font=font, anchor="mb")

out_path = os.path.join(os.path.dirname(__file__), "sample.png")
img.save(out_path, "PNG")
print(f"Generated: {out_path}  ({W}x{H})")