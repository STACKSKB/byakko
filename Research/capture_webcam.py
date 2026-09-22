"""Single still from the user-authorized keyboard camera; no audio capture."""
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent / 'extracted' / 'camera-python'))
import cv2

camera = cv2.VideoCapture(0, cv2.CAP_DSHOW)
try:
    if not camera.isOpened():
        raise RuntimeError('Camera 0 unavailable')
    camera.set(cv2.CAP_PROP_FRAME_WIDTH, 1280)
    camera.set(cv2.CAP_PROP_FRAME_HEIGHT, 720)
    for _ in range(20):
        ok, frame = camera.read()
        if not ok:
            raise RuntimeError('Camera frame unavailable')
    path = Path(__file__).parent / 'captures' / f'keyboard-camera-{time.time_ns()}.png'
    success, encoded = cv2.imencode('.png', frame)
    if not success:
        raise RuntimeError('PNG encode failed')
    with path.open('xb') as output:
        output.write(encoded.tobytes())
    print(path.resolve())
finally:
    camera.release()
