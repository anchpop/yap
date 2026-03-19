import { useState, useCallback } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import {
  ArrowRight,
  ArrowLeft,
  Check,
  Users,
  Globe,
  Video,
  Search,
  Youtube,
  HelpCircle,
  Clock,
  GraduationCap,
  Heart,
  Briefcase,
  Plane,
  Sparkles,
  SignalZero,
  SignalLow,
  SignalMedium,
  SignalHigh,
  Signal,
  Bell,
  type LucideIcon,
} from "lucide-react";
import type {
  Language,
  HeardAbout,
  Motivation,
  ExperienceLevel,
  StudyGoal,
  OnboardingSelections,
} from "../../../yap-frontend-rs/pkg/yap_frontend_rs";
import { nativeLanguageNames, languageFlags } from "@/lib/utils";
import { useOneSignalNotifications } from "@/hooks/use-onesignal-notifications";

// Re-export for use in LanguageSelector
export type { OnboardingSelections, HeardAbout };

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface OnboardingData {
  motivation: Motivation | null;
  experience: ExperienceLevel | null;
  studyGoal: StudyGoal | null;
}

interface OnboardingFlowProps {
  targetLanguage: Language;
  nativeLanguage: Language;
  hasHeardAbout: boolean;
  onHeardAbout: (value: HeardAbout) => void;
  onComplete: (selections: OnboardingSelections) => void;
  onBack: () => void;
}

// ---------------------------------------------------------------------------
// Forgetting Curve Chart
// ---------------------------------------------------------------------------

function calcDecay(startY: number, endY: number, width: number) {
  return -Math.log(endY / startY) / width;
}

function forgetting(startY: number, decay: number, t: number) {
  return startY * Math.exp(-decay * t);
}

function ForgettingCurveChart({ visibleCount }: { visibleCount: number }) {
  const W = 360;
  const H = 160;
  const topPad = 20;
  const botPad = 10;
  const chartH = H - topPad - botPad;

  const segments = [
    { startY: 1.0, endY: 0.28, widthFrac: 0.22, color: "var(--chart-1)", fillColor: "color-mix(in oklch, var(--chart-1) 15%, transparent)" },
    { startY: 1.0, endY: 0.38, widthFrac: 0.30, color: "var(--chart-2)", fillColor: "color-mix(in oklch, var(--chart-2) 15%, transparent)" },
    { startY: 1.0, endY: 0.55, widthFrac: 0.48, color: "var(--chart-3)", fillColor: "color-mix(in oklch, var(--chart-3) 15%, transparent)" },
  ];

  // Build curve paths
  let offsetX = 0;
  const curves: Array<{
    linePath: string;
    fillPath: string;
    color: string;
    fillColor: string;
    reviewX: number;
    reviewY: number;
  }> = [];

  for (const seg of segments) {
    const segWidth = W * seg.widthFrac;
    const decay = calcDecay(seg.startY, seg.endY, segWidth);
    const startX = offsetX;

    const points: Array<[number, number]> = [];
    const steps = 60;
    for (let s = 0; s <= steps; s++) {
      const t = (s / steps) * segWidth;
      const y = forgetting(seg.startY, decay, t);
      const px = startX + t;
      const py = topPad + (1 - y) * chartH;
      points.push([px, py]);
    }

    const linePath = points.map((p, j) => `${j === 0 ? "M" : "L"}${p[0].toFixed(1)},${p[1].toFixed(1)}`).join(" ");
    const fillPath = linePath + ` L${points[points.length - 1][0].toFixed(1)},${topPad + chartH} L${startX},${topPad + chartH} Z`;

    curves.push({
      linePath,
      fillPath,
      color: seg.color,
      fillColor: seg.fillColor,
      reviewX: startX,
      reviewY: topPad,
    });

    offsetX += segWidth;
  }

  const gridLines = [0.25, 0.5, 0.75].map((frac) => topPad + frac * chartH);

  return (
    <svg viewBox={`0 0 ${W} ${H + 20}`} className="w-full max-w-sm mx-auto" aria-label="Forgetting curve chart showing how spaced repetition helps memory">
      {/* Grid lines */}
      {gridLines.map((y, i) => (
        <line key={i} x1={0} y1={y} x2={W} y2={y} stroke="#e5e5e5" strokeWidth={1} strokeDasharray="4 4" />
      ))}

      {/* Curves — only render up to visibleCount */}
      <AnimatePresence>
        {curves.slice(0, visibleCount).map((curve, i) => (
          <motion.g
            key={i}
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
          >
            <motion.path
              d={curve.fillPath}
              fill={curve.fillColor}
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              transition={{ duration: 0.6 }}
            />
            <motion.path
              d={curve.linePath}
              fill="none"
              stroke={curve.color}
              strokeWidth={2.5}
              strokeLinecap="round"
              initial={{ pathLength: 0 }}
              animate={{ pathLength: 1 }}
              transition={{ duration: 0.8, ease: "easeOut" }}
            />
            {/* Review point dot + dashed line for curves after the first */}
            {i > 0 && (
              <>
                <motion.line
                  x1={curve.reviewX}
                  y1={topPad + chartH}
                  x2={curve.reviewX}
                  y2={curve.reviewY}
                  stroke={curve.color}
                  strokeWidth={1.5}
                  strokeDasharray="4 3"
                  initial={{ opacity: 0 }}
                  animate={{ opacity: 1 }}
                  transition={{ duration: 0.3 }}
                />
                <motion.circle
                  cx={curve.reviewX}
                  cy={curve.reviewY}
                  r={6}
                  fill="white"
                  stroke={curve.color}
                  strokeWidth={2}
                  initial={{ scale: 0 }}
                  animate={{ scale: 1 }}
                  transition={{ delay: 0.2, type: "spring", stiffness: 300, damping: 15 }}
                />
              </>
            )}
          </motion.g>
        ))}
      </AnimatePresence>

      {/* TIME label */}
      <text x={W - 5} y={H + 12} textAnchor="end" className="fill-muted-foreground" fontSize={11} fontWeight={500}>
        TIME &rarr;
      </text>
    </svg>
  );
}

// ---------------------------------------------------------------------------
// Individual Screens
// ---------------------------------------------------------------------------

function ScreenWrapper({
  children,
  screenKey,
}: {
  children: React.ReactNode;
  screenKey: string;
}) {
  return (
    <div
      key={screenKey}
      className="w-full max-w-lg mx-auto flex flex-col items-center gap-6 animate-slide-in-from-right"
    >
      {children}
    </div>
  );
}

function OptionButton({
  label,
  selected,
  onClick,
  icon: Icon,
}: {
  label: string;
  selected: boolean;
  onClick: () => void;
  icon?: LucideIcon;
}) {
  return (
    <Button
      variant="outline"
      size="lg"
      onClick={onClick}
      className={`w-full text-left justify-between text-base py-6 transition-all ${
        selected
          ? "border-primary bg-primary/10 ring-2 ring-primary/30"
          : "hover:border-primary/40"
      }`}
    >
      <span className="flex items-center gap-3">
        {Icon && <Icon className="h-5 w-5 shrink-0 text-muted-foreground" />}
        {label}
      </span>
      {selected && <Check className="h-4 w-4 text-primary shrink-0" />}
    </Button>
  );
}

// Screen: SRS teaser
const srsStudies = [
  {
    authors: "Ebbinghaus, H.",
    year: 1885,
    title: "Memory: A Contribution to Experimental Psychology",
    journal: "Teachers College, Columbia University",
    url: "https://psychclassics.yorku.ca/Ebbinghaus/index.htm",
  },
  {
    authors: "Cepeda, N.J., Pashler, H., Vul, E., Wixted, J.T., & Rohrer, D.",
    year: 2006,
    title: "Distributed practice in verbal recall tasks: A review and quantitative synthesis",
    url: "https://doi.org/10.1037/0033-2909.132.3.354",
    journal: "Psychological Bulletin, 132(3), 354\u2013380",
  },
  {
    authors: "Karpicke, J.D. & Roediger, H.L.",
    year: 2008,
    title: "The Critical Importance of Retrieval for Learning",
    url: "https://doi.org/10.1126/science.1152408",
    journal: "Science, 319(5865), 966\u2013968",
  },
  {
    authors: "Ye, J.J., Su, J., & Cao, Y.",
    year: 2022,
    title: "A Stochastic Shortest Path Algorithm for Optimizing Spaced Repetition Scheduling",
    url: "https://dl.acm.org/doi/10.1145/3534678.3539081?cid=99660547150",
    journal: "KDD \u201922: Proceedings of the 28th ACM SIGKDD Conference, 4381\u20134390",
  },
];

function SrsTeaserScreen({ onNext }: { onNext: () => void }) {
  return (
    <ScreenWrapper screenKey="srs-teaser">
      <h2 className="text-3xl md:text-4xl font-bold text-center leading-snug" style={{ textWrap: "balance" }}>
        Yap is based on one scientifically proven idea:
      </h2>
      <div className="w-full relative h-64 select-none overflow-x-clip" aria-hidden>
        {srsStudies.map((study, i) => (
          <motion.a
            key={study.title}
            href={study.url}
            target="_blank"
            rel="noopener noreferrer"
            initial={{ y: 60, opacity: 0 }}
            animate={{ y: 0, opacity: 1, rotate: (i - 1.5) * 2 }}
            transition={{ delay: 0.3 + i * 0.5, duration: 0.5, ease: "easeOut" }}
            className="absolute inset-x-4 bg-white text-black rounded shadow-md px-5 py-4 border border-neutral-200 block"
            style={{ zIndex: i, top: `${i * 8}px`, height: "200px" }}
          >
            <p className="text-[11px] font-semibold leading-tight truncate">{study.title}</p>
            <p className="text-[9px] text-neutral-500 mt-1 truncate">
              {study.authors} ({study.year}). <span className="italic">{study.journal}</span>
            </p>
            {/* Simulated text lines */}
            <div className="mt-3 flex flex-col gap-1.5">
              {Array.from({ length: 6 }).map((_, j) => (
                <div
                  key={j}
                  className="h-1.5 bg-neutral-200 rounded-full"
                  style={{ width: `${j === 5 ? 40 + (i * 10) % 30 : 75 + ((i + j) * 7) % 25}%` }}
                />
              ))}
            </div>
          </motion.a>
        ))}
      </div>
      <div className="-mt-2 h-10 flex flex-col items-center justify-start">
        <motion.h3
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={{ delay: 0.3 + srsStudies.length * 0.5 + 0.3, duration: 0.6 }}
          className="text-3xl font-bold italic text-accent-foreground"
        >
          Spaced repetition.
        </motion.h3>
      </div>
      <Button size="lg" onClick={onNext} className="px-8 text-base">
        Continue
        <ArrowRight className="h-4 w-4 ml-2" />
      </Button>
    </ScreenWrapper>
  );
}

// Screen: SRS Intro
function SrsIntroScreen({ onNext }: { onNext: () => void }) {
  // 0 = nothing shown, 1/2/3 = curves visible, 4 = "word learned" state
  const [reviewCount, setReviewCount] = useState(0);
  const totalCurves = 3;
  const learned = reviewCount > totalCurves;

  const dotColors = ["var(--chart-1)", "var(--chart-2)", "var(--chart-3)"];

  return (
    <ScreenWrapper screenKey="srs-intro">
      <Card className="w-full p-8 flex flex-col items-center gap-4" animate>
        <p className="text-sm font-semibold text-primary tracking-wide uppercase self-start">
          How Yap works
        </p>
        <h2 className="text-2xl md:text-3xl font-bold text-left self-start leading-snug">
          Every time you review a word, you'll remember it for{" "}
          <span className="italic text-primary">longer.</span>
        </h2>

        {/* Review dots legend — only show dots for completed reviews */}
        <div className="flex items-center gap-2 self-start h-5">
          {dotColors.slice(0, Math.min(reviewCount, totalCurves)).map((color, i) => (
            <motion.span
              key={i}
              className="w-3 h-3 rounded-full"
              style={{ backgroundColor: color }}
              initial={{ scale: 0 }}
              animate={{ scale: 1 }}
              transition={{ type: "spring", stiffness: 300, damping: 15 }}
            />
          ))}
          {reviewCount > 0 && !learned && (
            <span className="text-sm text-muted-foreground ml-1">
              {reviewCount === 1 ? "review" : "reviews"}
            </span>
          )}
        </div>

        <AnimatePresence mode="wait">
          {learned ? (
            <motion.div
              key="learned"
              initial={{ opacity: 0, scale: 0.9 }}
              animate={{ opacity: 1, scale: 1 }}
              className="flex flex-col items-center gap-3 py-8"
            >
              <div className="text-4xl font-bold flex items-center gap-2">
                <Check className="h-8 w-8 text-primary" /> Word learned
              </div>
              <p className="text-muted-foreground">
                That word is now in long-term memory!
              </p>
            </motion.div>
          ) : (
            <motion.div key="chart" exit={{ opacity: 0 }}>
              <ForgettingCurveChart visibleCount={reviewCount} />
            </motion.div>
          )}
        </AnimatePresence>

        <Button
          size="lg"
          className="mt-2 px-8 text-base"
          onClick={() => {
            if (learned) {
              onNext();
            } else {
              setReviewCount((c) => c + 1);
            }
          }}
        >
          {learned ? (
            <>
              Continue
              <ArrowRight className="h-4 w-4 ml-2" />
            </>
          ) : (
            "Review"
          )}
        </Button>
      </Card>
    </ScreenWrapper>
  );
}

// Screen: SRS conclusion with growth chart
function GrowthChart() {
  const w = 280;
  const h = 160;
  const pad = { top: 10, right: 10, bottom: 30, left: 40 };
  const cw = w - pad.left - pad.right;
  const ch = h - pad.top - pad.bottom;

  // Quadratic growth curve: y = t^2
  const points = Array.from({ length: 50 }, (_, i) => {
    const t = i / 49;
    return { x: pad.left + t * cw, y: pad.top + ch - t * t * ch };
  });
  const linePath = points.map((p, i) => `${i === 0 ? "M" : "L"}${p.x},${p.y}`).join(" ");
  const areaPath = `${linePath} L${pad.left + cw},${pad.top + ch} L${pad.left},${pad.top + ch} Z`;

  return (
    <svg viewBox={`0 0 ${w} ${h}`} className="w-full max-w-xs">
      {/* Area fill */}
      <motion.path
        d={areaPath}
        fill="var(--chart-1)"
        fillOpacity={0.15}
        initial={{ clipPath: "inset(0 100% 0 0)" }}
        animate={{ clipPath: "inset(0 0% 0 0)" }}
        transition={{ duration: 1.5, ease: "easeOut", delay: 0.3 }}
      />
      {/* Line */}
      <motion.path
        d={linePath}
        fill="none"
        stroke="var(--chart-1)"
        strokeWidth={2.5}
        strokeLinecap="round"
        initial={{ pathLength: 0 }}
        animate={{ pathLength: 1 }}
        transition={{ duration: 1.5, ease: "easeOut", delay: 0.3 }}
      />
      {/* Axes */}
      <line x1={pad.left} y1={pad.top} x2={pad.left} y2={pad.top + ch} stroke="currentColor" strokeOpacity={0.2} />
      <line x1={pad.left} y1={pad.top + ch} x2={pad.left + cw} y2={pad.top + ch} stroke="currentColor" strokeOpacity={0.2} />
      {/* Labels */}
      <text x={pad.left + cw / 2} y={h - 4} textAnchor="middle" className="fill-muted-foreground text-[10px]">
        Days
      </text>
      <text x={12} y={pad.top + ch / 2} textAnchor="middle" className="fill-muted-foreground text-[10px]" transform={`rotate(-90, 12, ${pad.top + ch / 2})`}>
        Words learned
      </text>
    </svg>
  );
}

function SrsConclusionScreen({ onNext }: { onNext: () => void }) {
  return (
    <ScreenWrapper screenKey="srs-conclusion">
      <h2 className="text-3xl md:text-4xl font-bold text-center leading-snug" style={{ textWrap: "balance" }}>
        That's why if you study a little bit every day, you'll learn a lot.
      </h2>
      <GrowthChart />
      <Button size="lg" onClick={onNext} className="px-8 text-base">
        Set a goal
        <ArrowRight className="h-4 w-4 ml-2" />
      </Button>
    </ScreenWrapper>
  );
}

// Screen 2: How did you hear about Yap?
function HeardAboutScreen({
  onSelect,
}: {
  onSelect: (v: HeardAbout) => void;
}) {
  const [selected, setSelected] = useState<HeardAbout | null>(null);

  const options: Array<{ key: HeardAbout; label: string; icon: LucideIcon }> = [
    { key: "FriendsOrFamily", label: "Friends or family", icon: Users },
    { key: "Reddit", label: "Reddit", icon: Globe },
    { key: "TikTok", label: "TikTok", icon: Video },
    { key: "GoogleSearch", label: "Google Search", icon: Search },
    { key: "YouTube", label: "YouTube", icon: Youtube },
    { key: "Other", label: "Other", icon: HelpCircle },
  ];

  return (
    <ScreenWrapper screenKey="heard-about">
      <h2 className="text-3xl md:text-4xl font-bold text-center" style={{ textWrap: "balance" }}>
        How did you hear about Yap?
      </h2>
      <div className="flex flex-col gap-3 w-full">
        {options.map((opt) => (
          <OptionButton
            key={opt.key}
            label={opt.label}
            icon={opt.icon}
            selected={selected === opt.key}
            onClick={() => setSelected(opt.key)}
          />
        ))}
      </div>
      <Button
        size="lg"
        disabled={!selected}
        onClick={() => selected && onSelect(selected)}
        className="mt-2 px-8 text-base"
      >
        Continue
        <ArrowRight className="h-4 w-4 ml-2" />
      </Button>
    </ScreenWrapper>
  );
}

// Screen 3: Why are you learning?
function MotivationScreen({
  value,
  onChange,
  onNext,
  targetLanguage,
}: {
  value: Motivation | null;
  onChange: (v: Motivation) => void;
  onNext: () => void;
  targetLanguage: Language;
}) {
  const options: Array<{ key: Motivation; label: string; icon: LucideIcon }> = [
    { key: "SpendTimeProductively", label: "Spend time productively", icon: Clock },
    { key: "SupportMyEducation", label: "Support my education", icon: GraduationCap },
    { key: "ConnectWithPeople", label: "Connect with people", icon: Heart },
    { key: "BoostMyCareer", label: "Boost my career", icon: Briefcase },
    { key: "PrepareForTravel", label: "Prepare for travel", icon: Plane },
    { key: "JustForFun", label: "Just for fun", icon: Sparkles },
    { key: "Other", label: "Other", icon: HelpCircle },
  ];

  return (
    <ScreenWrapper screenKey="motivation">
      <h2 className="text-3xl md:text-4xl font-bold text-center" style={{ textWrap: "balance" }}>
        Why are you learning {nativeLanguageNames[targetLanguage]}?
      </h2>
      <div className="flex flex-col gap-3 w-full">
        {options.map((opt) => (
          <OptionButton
            key={opt.key}
            label={opt.label}
            icon={opt.icon}
            selected={value === opt.key}
            onClick={() => onChange(opt.key)}
          />
        ))}
      </div>
      <Button
        size="lg"
        disabled={!value}
        onClick={onNext}
        className="mt-2 px-8 text-base"
      >
        Continue
        <ArrowRight className="h-4 w-4 ml-2" />
      </Button>
    </ScreenWrapper>
  );
}

// Screen 4: Experience level
function ExperienceScreen({
  value,
  onChange,
  onNext,
  targetLanguage,
}: {
  value: ExperienceLevel | null;
  onChange: (v: ExperienceLevel) => void;
  onNext: () => void;
  targetLanguage: Language;
}) {
  const options: Array<{ key: ExperienceLevel; label: string; icon: LucideIcon }> = [
    { key: "New", label: `I'm new to ${nativeLanguageNames[targetLanguage]}`, icon: SignalZero },
    { key: "CommonWords", label: "I know some common words", icon: SignalLow },
    { key: "BasicConversations", label: "I can have basic conversations", icon: SignalMedium },
    { key: "VariousTopics", label: "I can talk about various topics", icon: SignalHigh },
    { key: "MostTopics", label: "I can discuss most topics in detail", icon: Signal },
  ];

  return (
    <ScreenWrapper screenKey="experience">
      <h2 className="text-3xl md:text-4xl font-bold text-center" style={{ textWrap: "balance" }}>
        How much {nativeLanguageNames[targetLanguage]} do you know?
      </h2>
      <div className="flex flex-col gap-3 w-full">
        {options.map((opt) => (
          <OptionButton
            key={opt.key}
            label={opt.label}
            icon={opt.icon}
            selected={value === opt.key}
            onClick={() => onChange(opt.key)}
          />
        ))}
      </div>
      <Button
        size="lg"
        disabled={!value}
        onClick={onNext}
        className="mt-2 px-8 text-base"
      >
        Continue
        <ArrowRight className="h-4 w-4 ml-2" />
      </Button>
    </ScreenWrapper>
  );
}

// Screen 5: What you can achieve
function AchievementsScreen({ onNext }: { onNext: () => void }) {
  const items = [
    { icon: "💬", text: "Converse with confidence" },
    { icon: "📚", text: "Build a large vocabulary" },
    { icon: "🔄", text: "Develop a lasting learning habit" },
  ];

  return (
    <ScreenWrapper screenKey="achievements">
      <h2 className="text-3xl md:text-4xl font-bold text-center" style={{ textWrap: "balance" }}>
        Here's what you can achieve
      </h2>
      <div className="flex flex-col gap-4 w-full">
        {items.map((item) => (
          <Card key={item.text} className="p-5 flex items-center gap-4" animate>
            <span className="text-3xl">{item.icon}</span>
            <span className="text-lg font-medium">{item.text}</span>
          </Card>
        ))}
      </div>
      <Button
        size="lg"
        onClick={onNext}
        className="mt-2 px-8 text-base"
      >
        Continue
        <ArrowRight className="h-4 w-4 ml-2" />
      </Button>
    </ScreenWrapper>
  );
}

// Screen 6: Study goal
function StudyGoalScreen({
  value,
  onChange,
  onNext,
}: {
  value: StudyGoal | null;
  onChange: (v: StudyGoal) => void;
  onNext: () => void;
}) {
  const goals: Array<{ key: StudyGoal; label: string; mins: number; icon: LucideIcon }> = [
    { key: "Casual", label: "Casual", mins: 5, icon: SignalLow },
    { key: "Regular", label: "Regular", mins: 10, icon: SignalMedium },
    { key: "Serious", label: "Serious", mins: 15, icon: SignalHigh },
    { key: "Intense", label: "Intense", mins: 20, icon: Signal },
  ];

  const selectedGoal = goals.find((g) => g.key === value);
  const estimatedWords = selectedGoal ? selectedGoal.mins * 5 : null;

  return (
    <ScreenWrapper screenKey="study-goal">

      <h2 className="text-3xl md:text-4xl font-bold text-center" style={{ textWrap: "balance" }}>
        Set a daily study goal
      </h2>
      <div className="flex flex-col gap-3 w-full">
        {goals.map((goal) => (
          <OptionButton
            key={goal.key}
            label={`${goal.mins} min/day — ${goal.label}`}
            icon={goal.icon}
            selected={value === goal.key}
            onClick={() => onChange(goal.key)}
          />
        ))}
      </div>
      <AnimatePresence mode="wait">
        {estimatedWords && (
          <motion.p
            key={estimatedWords}
            initial={{ opacity: 0, y: 5 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0 }}
            className="text-base text-primary font-medium text-center"
          >
            That's ~{estimatedWords} words in your first week!
          </motion.p>
        )}
      </AnimatePresence>
      <Button
        size="lg"
        disabled={!value}
        onClick={onNext}
        className="mt-2 px-8 text-base"
      >
        Continue
        <ArrowRight className="h-4 w-4 ml-2" />
      </Button>
    </ScreenWrapper>
  );
}

// Screen: Notification prompt
function NotificationScreen({ onNext }: { onNext: () => void }) {
  const { subscribe, isLoading } = useOneSignalNotifications();
  const [requested, setRequested] = useState(false);

  const handleEnable = async () => {
    setRequested(true);
    await subscribe();
    onNext();
  };

  return (
    <ScreenWrapper screenKey="notifications">
      <div className="flex flex-col items-center gap-2">
        <div className="rounded-full bg-primary/10 p-4">
          <Bell className="h-10 w-10 text-primary" />
        </div>
      </div>
      <h2 className="text-3xl md:text-4xl font-bold text-center leading-snug" style={{ textWrap: "balance" }}>
        We'll remind you to practice so it becomes a habit!
      </h2>
      <p className="text-muted-foreground text-center text-base">
        A small daily reminder makes it easy to stay consistent and reach your goals.
      </p>
      <div className="flex flex-col gap-3 w-full">
        <Button size="lg" onClick={handleEnable} disabled={isLoading || requested} className="w-full">
          {isLoading ? "Enabling..." : "Enable reminders"}
        </Button>
        <Button variant="ghost" size="lg" onClick={onNext} className="w-full text-muted-foreground">
          Not now
        </Button>
      </div>
    </ScreenWrapper>
  );
}

// Screen 7: Ready to start
const letsGoPhrases: Record<Language, string> = {
  French: "Allons-y !",
  Spanish: "¡Vamos!",
  Korean: "가자!",
  English: "Let's go!",
  German: "Los geht's!",
  Chinese: "走吧！",
  Japanese: "行こう！",
  Russian: "Пойдем!",
  Portuguese: "Vamos lá!",
  Italian: "Andiamo!",
};

function ReadyScreen({
  targetLanguage,
  experience,
  onComplete,
}: {
  targetLanguage: Language;
  experience: ExperienceLevel | null;
  onComplete: (startFromScratch: boolean) => void;
}) {
  const isNew = experience === "New";

  return (
    <ScreenWrapper screenKey="ready">
      <motion.div
        initial={{ scale: 0 }}
        animate={{ scale: 1 }}
        transition={{ type: "spring", stiffness: 200, damping: 15 }}
        className="text-8xl"
      >
        {languageFlags[targetLanguage]}
      </motion.div>

      <h2 className="text-3xl md:text-4xl font-bold text-center" style={{ textWrap: "balance" }}>
        {isNew
          ? "Let's start from the beginning!"
          : "Now let's find the best place to start"}
      </h2>

      {isNew ? (
        <p className="text-muted-foreground text-center text-lg">
          We'll build your {nativeLanguageNames[targetLanguage]} foundation step by step.
        </p>
      ) : (
        <p className="text-muted-foreground text-center text-lg" style={{ textWrap: "balance" }}>
          Since you already know some {nativeLanguageNames[targetLanguage]}, we can skip ahead to where you belong.
        </p>
      )}

      {isNew ? (
        <Button
          size="lg"
          className="mt-2 px-8 text-base bg-primary hover:bg-primary/90"
          onClick={() => onComplete(true)}
        >
          {letsGoPhrases[targetLanguage]}
          <ArrowRight className="h-4 w-4 ml-2" />
        </Button>
      ) : (
        <div className="flex flex-col gap-3 w-full max-w-sm">
          <Button
            size="lg"
            className="w-full bg-primary hover:bg-primary/90"
            onClick={() => onComplete(false)}
          >
            Find my level
            <ArrowRight className="h-4 w-4 ml-2" />
          </Button>
          <Button
            size="lg"
            variant="outline"
            className="w-full"
            onClick={() => onComplete(true)}
          >
            Start from scratch
          </Button>
        </div>
      )}
    </ScreenWrapper>
  );
}

// ---------------------------------------------------------------------------
// Progress Bar
// ---------------------------------------------------------------------------

function OnboardingProgress({ current, total }: { current: number; total: number }) {
  const pct = ((current + 1) / total) * 100;
  return (
    <div className="w-full max-w-lg mx-auto mb-6">
      <Progress value={pct} />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main Flow
// ---------------------------------------------------------------------------

export function OnboardingFlow({
  targetLanguage,
  hasHeardAbout,
  onHeardAbout,
  onComplete,
  onBack,
}: OnboardingFlowProps) {
  const [step, setStep] = useState(0);
  const [data, setData] = useState<OnboardingData>({
    motivation: null,
    experience: null,
    studyGoal: null,
  });

  const { isSupported: notificationsSupported, isSubscribed, isInitialized: notificationsInitialized } = useOneSignalNotifications();
  const showNotifications = notificationsInitialized && notificationsSupported && !isSubscribed;

  // Build ordered screen list, conditionally including optional screens
  const screens = [
    ...(!hasHeardAbout ? ["heard-about" as const] : []),
    "motivation",
    "experience",
    "achievements",
    "srs-teaser",
    "srs-intro",
    "srs-conclusion",
    "study-goal",
    ...(showNotifications ? ["notifications" as const] : []),
    "ready",
  ] as const;

  const totalSteps = screens.length;
  const currentScreen = screens[step];

  const next = useCallback(() => setStep((s) => Math.min(s + 1, totalSteps - 1)), [totalSteps]);
  const prev = useCallback(() => {
    if (step === 0) {
      onBack();
    } else {
      setStep((s) => s - 1);
    }
  }, [step, onBack]);

  const handleComplete = useCallback(
    (startFromScratch: boolean) => {
      onComplete({
        startingFresh: startFromScratch,
        motivation: data.motivation ?? undefined,
        experienceLevel: data.experience ?? undefined,
        studyGoal: data.studyGoal ?? undefined,
      });
    },
    [onComplete, data]
  );

  return (
    <div className="w-full flex flex-col items-center px-4 py-8">
      <OnboardingProgress current={step} total={totalSteps} />

      {/* Back button */}
      <div className="w-full max-w-lg mb-4">
        <Button variant="ghost" size="sm" onClick={prev} className="text-muted-foreground">
          <ArrowLeft className="h-4 w-4 mr-1" />
          Back
        </Button>
      </div>

      {/* Screen content */}
      {currentScreen === "srs-teaser" && <SrsTeaserScreen onNext={next} />}
        {currentScreen === "srs-intro" && <SrsIntroScreen onNext={next} />}
        {currentScreen === "srs-conclusion" && <SrsConclusionScreen onNext={next} />}
        {currentScreen === "heard-about" && (
          <HeardAboutScreen
            onSelect={(v) => {
              onHeardAbout(v);
              next();
            }}
          />
        )}
        {currentScreen === "motivation" && (
          <MotivationScreen
            value={data.motivation}
            onChange={(v) => setData((d) => ({ ...d, motivation: v }))}
            onNext={next}
            targetLanguage={targetLanguage}
          />
        )}
        {currentScreen === "experience" && (
          <ExperienceScreen
            value={data.experience}
            onChange={(v) => setData((d) => ({ ...d, experience: v }))}
            onNext={next}
            targetLanguage={targetLanguage}
          />
        )}
        {currentScreen === "achievements" && <AchievementsScreen onNext={next} />}
        {currentScreen === "study-goal" && (
          <StudyGoalScreen
            value={data.studyGoal}
            onChange={(v) => setData((d) => ({ ...d, studyGoal: v }))}
            onNext={next}
          />
        )}
        {currentScreen === "notifications" && <NotificationScreen onNext={next} />}
        {currentScreen === "ready" && (
          <ReadyScreen
            targetLanguage={targetLanguage}
            experience={data.experience}
            onComplete={handleComplete}
          />
        )}
    </div>
  );
}
