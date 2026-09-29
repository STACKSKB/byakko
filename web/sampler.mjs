// SPDX-License-Identifier: GPL-3.0-or-later
// Capture remains local. The chooser supplies exactly the screen/tab/window
// selected by the user; no microphone fallback or frame recording is involved.
export async function prepareSampler(source, options, codec, environment = globalThis) {
  const media = environment.navigator?.mediaDevices;
  if (!media?.getDisplayMedia) throw new Error("This browser does not support screen or shared-audio capture.");
  const audio = source !== "ScreenAverage";
  if (audio && source?.PlaybackAudio?.bands !== 32) throw new Error("Unsupported audio band count.");
  const stream = await media.getDisplayMedia({ video: true, audio,
    ...(audio ? { systemAudio: "include", windowAudio: "system" } : {}) });
  let context, projection, video, sourceNode, timer, ended = false, onFailure, stopping = null;
  const tracks = stream.getTracks();
  const stop = () => {
    if (stopping) return stopping;
    ended = true;
    stopping = Promise.resolve().then(async () => {
      environment.clearTimeout(timer);
      tracks.forEach(track => track.stop());
      if (video) { video.pause(); video.srcObject = null; }
      sourceNode?.disconnect();
      projection?.free(); projection = null;
      await context?.close(); context = null;
    });
    return stopping;
  };
  try {
    let sample;
    if (audio) {
      if (!stream.getAudioTracks().length) throw new Error("The selected source supplied no audio. Choose a source with Share audio enabled.");
      const AudioContext = environment.AudioContext ?? environment.webkitAudioContext;
      if (!AudioContext) throw new Error("Audio analysis is unavailable in this browser.");
      context = new AudioContext();
      await context.resume();
      if (context.state !== "running") throw new Error("The browser did not start audio analysis.");
      const analyser = context.createAnalyser();
      analyser.fftSize = 2048;
      sourceNode = context.createMediaStreamSource(stream);
      sourceNode.connect(analyser);
      projection = new codec.BrowserAudioBands(context.sampleRate);
      const samples = new Float32Array(analyser.fftSize);
      sample = () => {
        if (context.state !== "running") throw new Error("Shared audio analysis was suspended.");
        analyser.getFloatTimeDomainData(samples);
        return { Bands: [...projection.frame(samples)] };
      };
    } else {
      video = environment.document.createElement("video");
      video.muted = true; video.playsInline = true; video.srcObject = stream;
      await video.play();
      const canvas = environment.document.createElement("canvas");
      const point = options?.sampling === "point";
      canvas.width = point ? 1 : 16; canvas.height = point ? 1 : 9;
      const drawing = canvas.getContext("2d", { willReadFrequently: true });
      if (!drawing) throw new Error("Screen color sampling is unavailable.");
      sample = () => {
        if (!video.videoWidth || !video.videoHeight) throw new Error("The selected screen has no available frame.");
        if (point) {
          const x = pointCoordinate(options.x, video.videoWidth);
          const y = pointCoordinate(options.y, video.videoHeight);
          drawing.drawImage(video, x, y, 1, 1, 0, 0, 1, 1);
        } else drawing.drawImage(video, 0, 0, canvas.width, canvas.height);
        return { Rgb: meanRgb(drawing.getImageData(0, 0, canvas.width, canvas.height).data) };
      };
    }
    let captureLost = tracks.some(track => track.readyState === "ended");
    const lost = () => {
      captureLost = true;
      if (!ended) onFailure?.("The browser's shared capture ended.");
    };
    tracks.forEach(track => track.addEventListener("ended", lost));
    return {
      label: (audio ? stream.getAudioTracks()[0] : stream.getVideoTracks()[0])?.label ?? "Shared source",
      start(receive, failed) {
        onFailure = failed;
        if (captureLost) { failed("The browser's shared capture ended before startup completed."); return; }
        const tick = () => {
          if (ended) return;
          try { receive(sample()); }
          catch (error) { failed(String(error.message ?? error)); return; }
          timer = environment.setTimeout(tick, audio ? 30 : 40);
        };
        tick();
      },
      stop,
    };
  } catch (error) { await stop(); throw error; }
}

export function pointCoordinate(value, extent) {
  if (!Number.isInteger(value) || value < 0 || value > 1000 || extent <= 0) throw new Error("Screen point must be within 0–1000.");
  return Math.floor(value * (extent - 1) / 1000);
}
export function meanRgb(rgba) {
  if (!rgba.length || rgba.length % 4) throw new Error("No complete screen pixels were sampled.");
  const sum = [0, 0, 0];
  for (let i = 0; i < rgba.length; i += 4) for (let c = 0; c < 3; c++) sum[c] += rgba[i + c];
  return sum.map(value => Math.floor(value / (rgba.length / 4)));
}
