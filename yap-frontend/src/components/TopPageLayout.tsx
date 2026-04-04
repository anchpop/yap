import type { ReactNode } from "react";
import { Header } from "@/components/header";
import { supabase } from "@/lib/supabase";
import type { UserInfo } from "@/App";
import type { Language } from "../../../yap-frontend-rs/pkg";

interface TopPageLayoutProps {
  userInfo: UserInfo | undefined;
  children: ReactNode;
  headerProps?: {
    onChangeLanguage?: () => void;
    showSignupNag?: boolean;
    language?: Language;
    backButton?: {
      label: string;
      onBack: () => void;
    };
    title?: string;
    dueCount?: number;
    dailyGoalPercent?: number;
  };
}

export function TopPageLayout({
  userInfo,
  children,
  headerProps = {},
}: TopPageLayoutProps) {
  return (
    <div className="flex flex-col py-2" style={{ minHeight: "calc(100dvh)" }}>
      <Header
        userInfo={userInfo}
        onSignOut={() => supabase.auth.signOut()}
        {...headerProps}
      />
      {children}
    </div>
  );
}
