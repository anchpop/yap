export function AudioVisualizer() {
  return (
    <span
      className="inline-flex items-center justify-center p-2 border border-primary/20 rounded-lg bg-primary/5"
      style={{ contain: 'strict', width: '80px', height: '48px' }}
    >
      <svg width="48" height="32" viewBox="0 0 48 32" className="inline-block">
        <g fill="currentColor" className="text-primary opacity-60">
          <rect x="2" y="8" width="4" height="16" rx="2">
            <animate attributeName="height" values="16;24;16" dur="1.2s" repeatCount="indefinite" />
            <animate attributeName="y" values="8;4;8" dur="1.2s" repeatCount="indefinite" />
          </rect>
          <rect x="10" y="4" width="4" height="24" rx="2">
            <animate attributeName="height" values="24;12;24" dur="1.2s" repeatCount="indefinite" begin="0.2s" />
            <animate attributeName="y" values="4;10;4" dur="1.2s" repeatCount="indefinite" begin="0.2s" />
          </rect>
          <rect x="18" y="6" width="4" height="20" rx="2">
            <animate attributeName="height" values="20;28;20" dur="1.2s" repeatCount="indefinite" begin="0.4s" />
            <animate attributeName="y" values="6;2;6" dur="1.2s" repeatCount="indefinite" begin="0.4s" />
          </rect>
          <rect x="26" y="4" width="4" height="24" rx="2">
            <animate attributeName="height" values="24;16;24" dur="1.2s" repeatCount="indefinite" begin="0.6s" />
            <animate attributeName="y" values="4;8;4" dur="1.2s" repeatCount="indefinite" begin="0.6s" />
          </rect>
          <rect x="34" y="10" width="4" height="12" rx="2">
            <animate attributeName="height" values="12;20;12" dur="1.2s" repeatCount="indefinite" begin="0.8s" />
            <animate attributeName="y" values="10;6;10" dur="1.2s" repeatCount="indefinite" begin="0.8s" />
          </rect>
          <rect x="42" y="8" width="4" height="16" rx="2">
            <animate attributeName="height" values="16;24;16" dur="1.2s" repeatCount="indefinite" begin="1s" />
            <animate attributeName="y" values="8;4;8" dur="1.2s" repeatCount="indefinite" begin="1s" />
          </rect>
        </g>
      </svg>
    </span>
  );
}
