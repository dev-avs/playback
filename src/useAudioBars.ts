import { useEffect, useRef, useState } from "react";

/**
 * Bars in the meter. Kept well below a real spectrum analyser's — this sits
 * in a 350px widget next to the track title, not on its own panel.
 */
export const BAR_COUNT = 5;

/**
 * Bar height range in CSS pixels. These mirror `.art-mini-bars` in App.css:
 * the minimum equals the bar width, so a silent bar is a round dot rather than
 * a squashed pill.
 */
const BAR_MIN = 3;
const BAR_MAX = 14;

/**
 * Band edges in Hz, log-spaced the way pitch is heard — each band is ~3x the
 * one below it. Given in Hz rather than FFT bins because loopback runs at
 * whatever the output device does, so the bin width is not known until the
 * AudioContext exists.
 */
const BAND_HZ = [40, 118, 350, 1035, 3060, 9000];

/**
 * Drives the art-mode visualiser from the system audio mix.
 *
 * We only mirror another app's playback, so there is no audio element to tap.
 * Electron's loopback capture gives us the output mix instead (see the
 * setDisplayMediaRequestHandler in main.cjs), which is fed through an
 * AnalyserNode and reduced to a handful of frequency bands.
 *
 * Heights are written straight to the DOM: at 60 fps, routing them through
 * React state would re-render the whole widget every frame.
 *
 * @returns refs to attach to the bar elements, and whether live audio is
 *   actually driving them (false means the caller should keep its CSS fallback)
 */
export function useAudioBars(active: boolean) {
  const barRefs = useRef<(HTMLSpanElement | null)[]>(
    Array<HTMLSpanElement | null>(BAR_COUNT).fill(null),
  );
  const [live, setLive] = useState(false);

  useEffect(() => {
    if (!active) return;

    let cancelled = false;
    let raf = 0;
    let stream: MediaStream | null = null;
    let ctx: AudioContext | null = null;
    const bars = barRefs.current;

    const start = async () => {
      try {
        stream = await navigator.mediaDevices.getDisplayMedia({
          audio: true,
          video: true,
        });
      } catch {
        return; // permission denied or unsupported — CSS fallback stays
      }
      if (cancelled) {
        stream?.getTracks().forEach((t) => t.stop());
        return;
      }

      // the video track only existed to unlock loopback audio
      stream.getVideoTracks().forEach((t) => t.stop());
      const audio = stream.getAudioTracks();
      if (!audio.length) return;

      ctx = new AudioContext();
      const source = ctx.createMediaStreamSource(new MediaStream(audio));
      const analyser = ctx.createAnalyser();
      // 2048, not 256. At a 48 kHz mix 256 bins sit ~187 Hz apart, so the whole
      // bass range below 250 Hz collapses into the first bin or two and no band
      // edge can address it — the previous edges started at bin 2, i.e. 375 Hz,
      // so the meter never saw bass and every bar tracked the mids in unison.
      // At 2048 a bin is ~23 Hz and the bands mean what they say.
      analyser.fftSize = 2048;
      // Some smoothing here, the rest in the attack/decay below; too much in
      // the analyser alone makes every bar lag the beat.
      analyser.smoothingTimeConstant = 0.7;
      source.connect(analyser);

      const bins = new Uint8Array(analyser.frequencyBinCount);
      const hzPerBin = ctx.sampleRate / analyser.fftSize;
      const edges = BAND_HZ.map((hz) =>
        Math.max(0, Math.min(bins.length, Math.round(hz / hzPerBin))),
      );

      const shown = new Array(BAR_COUNT).fill(0);
      // Last height written per bar, so an unchanged frame touches no style.
      const written = new Array(BAR_COUNT).fill(-1);

      setLive(true);

      const draw = () => {
        analyser.getByteFrequencyData(bins);
        for (let b = 0; b < BAR_COUNT; b++) {
          let sum = 0;
          let peak = 0;
          const from = edges[b];
          const to = Math.min(edges[b + 1], bins.length);
          for (let i = from; i < to; i++) {
            sum += bins[i];
            if (bins[i] > peak) peak = bins[i];
          }
          // The mean alone flattens a band the wider it gets, and these widen
          // sharply towards the top — the last band spans hundreds of bins, most
          // of them silent at any instant, so its mean sits near zero however
          // loud the music is. Folding in the band's peak keeps a real transient
          // visible without letting one bin make the bar twitch.
          const mean = to > from ? sum / (to - from) : 0;
          const level = (mean * 0.6 + peak * 0.4) / 255;

          // ease upward fast, fall back slowly — reads as a level meter
          shown[b] =
            level > shown[b] ? level * 0.6 + shown[b] * 0.4 : shown[b] * 0.86 + level * 0.14;

          const el = bars[b];
          if (!el) continue;
          // Continuous height, so the pills glide. Quarter-pixel steps are
          // finer than the eye follows and keep still frames from touching style.
          const h =
            Math.round((BAR_MIN + Math.min(1, shown[b] * 1.25) * (BAR_MAX - BAR_MIN)) * 4) / 4;
          if (h !== written[b]) {
            el.style.height = `${h}px`;
            written[b] = h;
          }
        }
        raf = requestAnimationFrame(draw);
      };
      draw();
    };

    start();

    return () => {
      cancelled = true;
      cancelAnimationFrame(raf);
      stream?.getTracks().forEach((t) => t.stop());
      ctx?.close().catch(() => {});
      setLive(false);
      for (const el of bars) {
        if (el) el.style.height = "";
      }
    };
  }, [active]);

  return { barRefs, live };
}
