export type EventChallengeSolves = {
  event_id: string;
  challenge_id: string;
  user_id: string;
  team_id?: string;
  obtained_points: number;
  bonus_points: number;
  created_at: string;
};
