"""Short quiet synthesized test tone; no recording and no volume changes."""
import io
import math
import struct
import wave
import winsound

rate = 48000
duration = 6
buffer = io.BytesIO()
with wave.open(buffer, 'wb') as output:
    output.setnchannels(1)
    output.setsampwidth(2)
    output.setframerate(rate)
    # Smooth 20 ms attack/release avoids clicks. Peak amplitude is 4%.
    frames = bytearray()
    for i in range(rate * duration):
        fade = min(1.0, i / 960, (rate * duration - 1 - i) / 960)
        sample = round(32767 * 0.04 * fade * math.sin(2 * math.pi * 480 * i / rate))
        frames.extend(struct.pack('<h', sample))
    output.writeframes(frames)
winsound.PlaySound(buffer.getvalue(), winsound.SND_MEMORY | winsound.SND_NODEFAULT)
