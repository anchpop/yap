import { useEffect, useState } from "react";
import { useParams, useNavigate } from "react-router-dom";
import { useOutletContext } from "react-router-dom";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { TopPageLayout } from "@/components/TopPageLayout";
import { Skeleton } from "@/components/ui/skeleton";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";
import {
  get_profile_by_id,
  get_user_language_stats_by_id,
  update_profile,
  get_follow_status,
  follow_user,
  unfollow_user,
  type Profile,
  type UserLanguageStats,
  type FollowStatus,
} from "../../../yap-frontend-rs/pkg";
import { Pencil, X, Check } from "lucide-react";
import { toast } from "sonner";
import { getLanguageFlag, getLanguageName } from "@/lib/utils";
import type { AppContextType } from "@/App";

export function UserProfilePage() {
  const { id } = useParams<{ id: string }>();
  const { userInfo, accessToken } = useOutletContext<AppContextType>();
  const navigate = useNavigate();
  const [profile, setProfile] = useState<Profile | null>(null);
  const [languageStats, setLanguageStats] = useState<UserLanguageStats[]>([]);
  const [loading, setLoading] = useState(true);
  const [statsLoading, setStatsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [isEditing, setIsEditing] = useState(false);
  const [editDisplayName, setEditDisplayName] = useState("");
  const [editBio, setEditBio] = useState("");
  const [saving, setSaving] = useState(false);
  const [followStatus, setFollowStatus] = useState<FollowStatus | null>(null);
  const [followLoading, setFollowLoading] = useState(false);

  const isOwnProfile = userInfo?.id === id;

  useEffect(() => {
    async function loadProfile() {
      if (!id) {
        setError("No user ID provided");
        setLoading(false);
        return;
      }

      try {
        setLoading(true);
        const profileData = await get_profile_by_id(id);
        setProfile(profileData);
        setEditDisplayName(profileData.display_name || "");
        setEditBio(profileData.bio || "");
        setError(null);
      } catch (err) {
        console.error("Error loading profile:", err);
        setError("Failed to load profile");
      } finally {
        setLoading(false);
      }
    }

    loadProfile();
  }, [id]);

  useEffect(() => {
    async function loadLanguageStats() {
      if (!id) {
        setStatsLoading(false);
        return;
      }

      try {
        setStatsLoading(true);
        const stats = await get_user_language_stats_by_id(id);
        setLanguageStats(stats);
      } catch (err) {
        console.error("Error loading language stats:", err);
        // Don't set error state - stats are optional
        setLanguageStats([]);
      } finally {
        setStatsLoading(false);
      }
    }

    loadLanguageStats();
  }, [id]);

  useEffect(() => {
    async function loadFollowStatus() {
      if (!id || !accessToken) {
        return;
      }

      try {
        const status = await get_follow_status(id, null, accessToken);
        setFollowStatus(status);
      } catch (err) {
        console.error("Error loading follow status:", err);
      }
    }

    loadFollowStatus();
  }, [id, accessToken]);

  const handleSave = async () => {
    if (!accessToken) {
      toast.error("You must be logged in to edit your profile");
      return;
    }

    try {
      setSaving(true);
      await update_profile(
        editDisplayName || null,
        editBio || null,
        accessToken,
      );

      // Reload profile to get updated data including slug
      const profileData = await get_profile_by_id(id!);
      setProfile(profileData);
      setIsEditing(false);
      toast.success("Profile updated successfully");
    } catch (err) {
      console.error("Error updating profile:", err);
      toast.error("Failed to update profile");
    } finally {
      setSaving(false);
    }
  };

  const handleCancel = () => {
    setEditDisplayName(profile?.display_name || "");
    setEditBio(profile?.bio || "");
    setIsEditing(false);
  };

  const handleFollow = async () => {
    if (!accessToken || !id) {
      toast.error("You must be logged in to follow users");
      return;
    }

    try {
      setFollowLoading(true);
      await follow_user(id, accessToken);
      setFollowStatus((prev) =>
        prev
          ? {
              ...prev,
              is_following: true,
              follower_count: prev.follower_count + 1,
            }
          : null,
      );
      toast.success("Followed successfully");
    } catch (err) {
      console.error("Error following user:", err);
      toast.error("Failed to follow user");
    } finally {
      setFollowLoading(false);
    }
  };

  const handleUnfollow = async () => {
    if (!accessToken || !id) {
      toast.error("You must be logged in to unfollow users");
      return;
    }

    try {
      setFollowLoading(true);
      await unfollow_user(id, accessToken);
      setFollowStatus((prev) =>
        prev
          ? {
              ...prev,
              is_following: false,
              follower_count: prev.follower_count - 1,
            }
          : null,
      );
      toast.success("Unfollowed successfully");
    } catch (err) {
      console.error("Error unfollowing user:", err);
      toast.error("Failed to unfollow user");
    } finally {
      setFollowLoading(false);
    }
  };

  if (loading) {
    return (
      <TopPageLayout
        userInfo={userInfo}
        headerProps={{
          backButton: { label: "Profile", onBack: () => navigate("/") },
        }}
      >
        <div className="space-y-4">
          <Card>
            <CardHeader>
              <Skeleton className="h-8 w-48" />
            </CardHeader>
            <CardContent className="space-y-3">
              <Skeleton className="h-4 w-full" />
              <Skeleton className="h-4 w-3/4" />
            </CardContent>
          </Card>
        </div>
      </TopPageLayout>
    );
  }

  if (error || !profile) {
    return (
      <TopPageLayout
        userInfo={userInfo}
        headerProps={{
          backButton: { label: "Profile", onBack: () => navigate("/") },
        }}
      >
        <Card>
          <CardContent className="pt-6">
            <div className="text-center text-muted-foreground">
              {error || "Profile not found"}
            </div>
          </CardContent>
        </Card>
      </TopPageLayout>
    );
  }

  return (
    <TopPageLayout
      userInfo={userInfo}
      headerProps={{
        backButton: { label: "Profile", onBack: () => navigate("/") },
      }}
    >
      <div className="space-y-4">
        <Card>
          <CardHeader>
            <div className="flex items-start justify-between">
              <div className="flex-1">
                {isEditing ? (
                  <div className="space-y-3">
                    <div className="flex flex-col gap-2">
                      <Label htmlFor="displayName">Display Name</Label>
                      <Input
                        id="displayName"
                        value={editDisplayName}
                        onChange={(e) => setEditDisplayName(e.target.value)}
                        placeholder="Enter your display name"
                        maxLength={50}
                      />
                    </div>
                  </div>
                ) : (
                  <>
                    <h2 className="text-3xl font-bold">
                      {profile.display_name || "Anonymous User"}
                    </h2>
                    {profile.display_name_slug && (
                      <p className="text-sm text-muted-foreground mt-1">
                        @{profile.display_name_slug}
                      </p>
                    )}
                    <div className="flex gap-4 mt-2 text-sm animate-fade-in-delay-2">
                      <span>
                        {followStatus ? (
                          <>
                            <strong>{followStatus.follower_count}</strong>{" "}
                            followers
                          </>
                        ) : (
                          <span className="invisible">0 followers</span>
                        )}
                      </span>
                      <span>
                        {followStatus ? (
                          <>
                            <strong>{followStatus.following_count}</strong>{" "}
                            following
                          </>
                        ) : (
                          <span className="invisible">0 following</span>
                        )}
                      </span>
                    </div>
                  </>
                )}
              </div>
              {!isOwnProfile && !isEditing && followStatus && (
                <Button
                  variant={followStatus.is_following ? "outline" : "default"}
                  onClick={
                    followStatus.is_following ? handleUnfollow : handleFollow
                  }
                  disabled={followLoading}
                  className="ml-2"
                >
                  {followStatus.is_following ? "Unfollow" : "Follow"}
                </Button>
              )}
              {isOwnProfile && !isEditing && (
                <Button
                  variant="ghost"
                  size="icon"
                  onClick={() => setIsEditing(true)}
                  className="ml-2"
                >
                  <Pencil className="h-4 w-4" />
                </Button>
              )}
              {isOwnProfile && isEditing && (
                <div className="flex gap-2 ml-2">
                  <Button
                    variant="ghost"
                    size="icon"
                    onClick={handleCancel}
                    disabled={saving}
                  >
                    <X className="h-4 w-4" />
                  </Button>
                  <Button
                    variant="default"
                    size="icon"
                    onClick={handleSave}
                    disabled={saving}
                  >
                    <Check className="h-4 w-4" />
                  </Button>
                </div>
              )}
            </div>
          </CardHeader>
          <CardContent>
            <div className="space-y-3">
              {isEditing ? (
                <div className="flex flex-col gap-2">
                  <Label htmlFor="bio">Bio</Label>
                  <Textarea
                    id="bio"
                    value={editBio}
                    onChange={(e) => setEditBio(e.target.value)}
                    placeholder="Tell us about yourself"
                    rows={4}
                    maxLength={500}
                  />
                  <p className="text-xs text-muted-foreground">
                    {editBio.length}/500 characters
                  </p>
                </div>
              ) : (
                <>
                  {profile.bio ? (
                    <p className="text-foreground whitespace-pre-wrap">
                      {profile.bio}
                    </p>
                  ) : (
                    <p className="text-muted-foreground italic">No bio yet</p>
                  )}
                </>
              )}

              <p className="text-xs text-muted-foreground pt-2 border-t">
                Member since{" "}
                {new Date(profile.created_at).toLocaleDateString("en-US", {
                  year: "numeric",
                  month: "long",
                  day: "numeric",
                })}
              </p>
            </div>
          </CardContent>
        </Card>

        {/* Language Stats Card */}
        {statsLoading ? (
          <Card>
            <CardHeader>
              <Skeleton className="h-6 w-40" />
            </CardHeader>
            <CardContent>
              <Skeleton className="h-20 w-full" />
            </CardContent>
          </Card>
        ) : languageStats.length > 0 ? (
          <Card>
            <CardHeader>
              <h3 className="text-xl font-semibold">Languages</h3>
            </CardHeader>
            <CardContent>
              <div className="space-y-4">
                {languageStats.map((stats) => (
                  <div key={stats.language} className="border rounded-lg p-4">
                    <div className="flex items-center gap-2 mb-3">
                      <span className="text-2xl">
                        {getLanguageFlag(stats.language)}
                      </span>
                      <h4 className="text-lg font-semibold">
                        {getLanguageName(stats.language)}
                      </h4>
                    </div>
                    <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                      <div>
                        <p className="text-sm text-muted-foreground">Cards</p>
                        <p className="text-2xl font-bold">
                          {stats.total_count}
                        </p>
                      </div>
                      <div>
                        <p className="text-sm text-muted-foreground">
                          Daily Streak
                        </p>
                        <p className="text-2xl font-bold">
                          {stats.daily_streak} 🔥
                        </p>
                      </div>
                      <div>
                        <p className="text-sm text-muted-foreground">XP</p>
                        <p className="text-2xl font-bold">
                          {Math.round(stats.xp)}
                        </p>
                      </div>
                      <div>
                        <p className="text-sm text-muted-foreground">Mastery</p>
                        <p className="text-2xl font-bold">
                          {stats.percent_known.toFixed(1)}%
                        </p>
                      </div>
                    </div>
                    <p className="text-xs text-muted-foreground mt-3 pt-3 border-t">
                      Learning since{" "}
                      {new Date(stats.started).toLocaleDateString("en-US", {
                        year: "numeric",
                        month: "long",
                        day: "numeric",
                      })}
                    </p>
                  </div>
                ))}
              </div>
            </CardContent>
          </Card>
        ) : null}
      </div>
    </TopPageLayout>
  );
}
