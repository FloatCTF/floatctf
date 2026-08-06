export type Discussions = {
    id: string;
    title: string;
    content: string;
    author_id: string;
    author_nickname: string;
    author_avatar?: string;
    is_liked: boolean;
    view_count: number;
    like_count: number;
    comment_count: number;
    created_at: string;
    updated_at: string;
};

export type DiscussionComments = {
    id: string;
    discussion_id: string;
    author_id: string;
    content: string;
    parent_id?: string;
    created_at: string;
    updated_at: string;
};
