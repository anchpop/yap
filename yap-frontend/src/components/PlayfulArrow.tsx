interface PlayfulArrowProps {
  direction: "up" | "down" | "left" | "right";
  flipStart?: boolean;
  className?: string;
  size?: number;
  opacity?: number;
}

export function PlayfulArrow({
  direction,
  flipStart = false,
  className = "",
  size = 100,
  opacity = 0.5,
}: PlayfulArrowProps) {
  const getTransform = () => {
    const transforms: string[] = [];

    // Base SVG points right from bottom-left
    // Apply direction rotation
    if (direction === "left") {
      transforms.push("scaleX(-1)");
    } else if (direction === "up") {
      transforms.push("rotate(-90deg)");
    } else if (direction === "down") {
      transforms.push("rotate(90deg)");
    }

    // Flip the start point if requested
    if (flipStart) {
      transforms.push("scaleY(-1)");
    }

    return transforms.join(" ");
  };

  // Calculate dimensions based on direction
  const isVertical = direction === "up" || direction === "down";
  const width = isVertical ? size * 0.75 : size;
  const height = isVertical ? size : size * 0.75;

  return (
    <svg
      width={width}
      height={height}
      viewBox="0 0 168 81"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      className={`playful-arrow ${className}`}
      style={{ transform: getTransform(), opacity }}
      overflow="visible"
    >
      <path
        d="M 0 81 C 0 -35 85 2 95 36 C 102 71 58 72 56 53 C 51 8 110 34 155 32"
        stroke="currentColor"
        strokeWidth="10"
        strokeLinecap="round"
        strokeLinejoin="round"
      />

      <path
        d="M 137 14 L 160 32 L 137 50"
        stroke="currentColor"
        strokeWidth="10"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}
