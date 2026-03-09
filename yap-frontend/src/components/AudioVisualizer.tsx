import { useRef, useEffect } from "react";
import { getFrequencyData } from "@/lib/audio-analyser";

interface AudioVisualizerProps {
  isPlaying: boolean;
}

const SIZE = 80;
const CENTER = SIZE / 2;
const BASE_RADIUS = 28;
const MAX_DISPLACEMENT = 10;
const NUM_POINTS = 64;
// How quickly the waveform settles back to a circle (0-1, lower = slower)
const LERP_SPEED = 0.08;

export function AudioVisualizer({ isPlaying }: AudioVisualizerProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const animFrameRef = useRef<number>(0);
  const isPlayingRef = useRef(isPlaying);
  // Store current radii for smooth interpolation
  const currentRadii = useRef<Float32Array>(
    new Float32Array(NUM_POINTS).fill(BASE_RADIUS)
  );

  // Keep ref in sync
  useEffect(() => {
    isPlayingRef.current = isPlaying;
  }, [isPlaying]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    // Set up canvas for HiDPI
    const dpr = window.devicePixelRatio || 1;
    canvas.width = SIZE * dpr;
    canvas.height = SIZE * dpr;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    function draw() {
      const playing = isPlayingRef.current;

      // Get target radii based on frequency data
      const targetRadii = new Float32Array(NUM_POINTS).fill(BASE_RADIUS);

      if (playing) {
        const freqData = getFrequencyData();
        if (freqData.length > 0) {
          for (let i = 0; i < NUM_POINTS; i++) {
            // Sample from lower frequencies more (they're more interesting)
            const freqIndex = Math.floor(
              (i / NUM_POINTS) * (freqData.length * 0.6)
            );
            const amplitude = freqData[freqIndex];
            targetRadii[i] = BASE_RADIUS + amplitude * MAX_DISPLACEMENT;
          }
        }
      }

      // Smoothly interpolate current radii toward target
      const radii = currentRadii.current;
      for (let i = 0; i < NUM_POINTS; i++) {
        radii[i] += (targetRadii[i] - radii[i]) * LERP_SPEED;
      }

      // Check if we're effectively at rest (all radii near base)
      const isAtRest =
        !playing && radii.every((r) => Math.abs(r - BASE_RADIUS) < 0.1);

      // Clear and draw
      ctx!.clearRect(0, 0, SIZE * dpr, SIZE * dpr);
      ctx!.save();
      ctx!.scale(dpr, dpr);

      const style = getComputedStyle(canvas!);
      const color = style.color;

      if (isAtRest) {
        ctx!.beginPath();
        ctx!.arc(CENTER, CENTER, BASE_RADIUS, 0, Math.PI * 2);
        ctx!.strokeStyle = color;
        ctx!.lineWidth = 2.5;
        ctx!.stroke();
      } else {
        ctx!.beginPath();

        const points: [number, number][] = [];
        for (let i = 0; i < NUM_POINTS; i++) {
          const angle = (i / NUM_POINTS) * Math.PI * 2 - Math.PI / 2;
          const r = radii[i];
          points.push([
            CENTER + Math.cos(angle) * r,
            CENTER + Math.sin(angle) * r,
          ]);
        }

        ctx!.moveTo(points[0][0], points[0][1]);

        // Smooth curve using Catmull-Rom to Bezier conversion
        for (let i = 0; i < NUM_POINTS; i++) {
          const p0 = points[(i - 1 + NUM_POINTS) % NUM_POINTS];
          const p1 = points[i];
          const p2 = points[(i + 1) % NUM_POINTS];
          const p3 = points[(i + 2) % NUM_POINTS];

          const tension = 6;
          const cp1x = p1[0] + (p2[0] - p0[0]) / tension;
          const cp1y = p1[1] + (p2[1] - p0[1]) / tension;
          const cp2x = p2[0] - (p3[0] - p1[0]) / tension;
          const cp2y = p2[1] - (p3[1] - p1[1]) / tension;

          ctx!.bezierCurveTo(cp1x, cp1y, cp2x, cp2y, p2[0], p2[1]);
        }

        ctx!.closePath();
        ctx!.strokeStyle = color;
        ctx!.lineWidth = 2.5;
        ctx!.stroke();
      }

      drawSpeakerIcon(ctx!, CENTER, CENTER, color);
      ctx!.restore();

      animFrameRef.current = requestAnimationFrame(draw);
    }

    animFrameRef.current = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(animFrameRef.current);
  }, []);

  return (
    <canvas
      ref={canvasRef}
      className="text-accent-foreground opacity-70"
      style={{ width: SIZE, height: SIZE }}
    />
  );
}

function drawSpeakerIcon(
  ctx: CanvasRenderingContext2D,
  cx: number,
  cy: number,
  color: string
) {
  ctx.save();
  ctx.translate(cx - 7, cy - 6);
  ctx.fillStyle = color;
  ctx.strokeStyle = color;
  ctx.lineWidth = 1.5;
  ctx.lineCap = "round";

  // Speaker body
  ctx.beginPath();
  ctx.moveTo(0, 4);
  ctx.lineTo(3, 4);
  ctx.lineTo(7, 1);
  ctx.lineTo(7, 11);
  ctx.lineTo(3, 8);
  ctx.lineTo(0, 8);
  ctx.closePath();
  ctx.fill();

  // Sound waves
  ctx.beginPath();
  ctx.arc(8, 6, 3, -Math.PI / 3, Math.PI / 3);
  ctx.stroke();

  ctx.beginPath();
  ctx.arc(8, 6, 6, -Math.PI / 3, Math.PI / 3);
  ctx.stroke();

  ctx.restore();
}
