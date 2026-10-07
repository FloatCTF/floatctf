export type DiscussionComments = {
  id: string;
  discussion_id: string;
  author_id: string;
  content: string;
  parent_id?: string;
  created_at: string;
  updated_at: string;
};
