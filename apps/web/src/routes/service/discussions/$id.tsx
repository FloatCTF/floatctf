import { ThumbsupIcon, CommentIcon, EyeIcon } from "@primer/octicons-react";
import { Avatar, Button } from "@primer/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import MDEditor from "@uiw/react-md-editor";
import { useTitle } from "ahooks";
import { useEffect, useState } from "react";

import { serviceApi } from "@/api";
import { MDPlusEditor, useMsgBanner } from "@/components";
import type { DiscussionComments, Discussions } from "@/entity";
import { DatetimeToShow } from "@/util";

export const Route = createFileRoute("/service/discussions/$id")({
    component: RouteComponent,
});

interface CommentWithReplies extends DiscussionComments {
    replies: CommentWithReplies[];
    author_nickname?: string;
    author_avatar?: string;
}

function buildCommentTree(
    comments: DiscussionComments[],
): CommentWithReplies[] {
    const map = new Map<string, CommentWithReplies>();
    const roots: CommentWithReplies[] = [];

    comments.forEach((c) => {
        map.set(c.id, { ...c, replies: [] });
    });

    comments.forEach((c) => {
        const node = map.get(c.id)!;
        if (c.parent_id && map.has(c.parent_id)) {
            map.get(c.parent_id)!.replies.push(node);
        } else {
            roots.push(node);
        }
    });

    roots.sort((a, b) => b.replies.length - a.replies.length);
    roots.forEach((r) => {
        r.replies.sort(
            (a, b) =>
                new Date(b.created_at).getTime() -
                new Date(a.created_at).getTime(),
        );
    });

    return roots;
}

function AvatarOrFallback({
    src,
    name,
    size,
}: {
    src?: string;
    name: string;
    size: number;
}) {
    if (src) return <Avatar src={src} size={size} />;
    return (
        <div
            className="shrink-0 flex items-center justify-center rounded-full bg-gray-200 text-gray-500 font-medium"
            style={{
                width: size,
                height: size,
                fontSize: size * 0.4,
            }}
        >
            {name?.[0]?.toUpperCase() || "?"}
        </div>
    );
}

function CommentEditor({
    value,
    setValue,
    avatar,
    nickname,
    onSubmit,
    onCancel,
    submitLabel,
}: {
    value: string;
    setValue: (v: string) => void;
    avatar?: string;
    nickname: string;
    onSubmit: () => void;
    onCancel?: () => void;
    submitLabel?: string;
}) {
    return (
        <div className="flex items-start gap-3">
            <AvatarOrFallback src={avatar} name={nickname} size={32} />
            <div className="flex-1 flex flex-col gap-2">
                <div className="text-xs text-gray-500 font-medium">
                    Comment as{" "}
                    <span className="text-gray-700">@{nickname}</span>
                </div>
                <MDPlusEditor height={200} value={value} setValue={setValue} />
                <div className="flex gap-2">
                    <Button size="small" variant="primary" onClick={onSubmit}>
                        {submitLabel || "Comment"}
                    </Button>
                    {onCancel && (
                        <Button size="small" onClick={onCancel}>
                            Cancel
                        </Button>
                    )}
                </div>
            </div>
        </div>
    );
}

function CommentItem({
    comment,
    discussionId,
    currentUserId,
    meAvatar,
    meNickname,
    parentNickname,
    onReply,
    onDelete,
}: {
    comment: CommentWithReplies;
    discussionId: string;
    currentUserId?: string;
    meAvatar?: string;
    meNickname: string;
    parentNickname?: string;
    onReply: (parentId: string, content: string) => void;
    onDelete: (commentId: string) => void;
}) {
    const [showReply, setShowReply] = useState(false);
    const [replyContent, setReplyContent] = useState("");
    const banner = useMsgBanner();

    const submitReply = () => {
        if (!replyContent.trim()) {
            banner.showBanner("critical", "Reply content cannot be empty");
            return;
        }
        onReply(comment.id, replyContent);
        setReplyContent("");
        setShowReply(false);
    };

    const authorName = comment.author_nickname || comment.author_id;

    return (
        <div className="border border-gray-200 rounded-md">
            {/* Header */}
            <div className="flex items-start gap-3 p-3 pb-0">
                <AvatarOrFallback
                    src={comment.author_avatar}
                    name={authorName}
                    size={32}
                />
                <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 flex-wrap">
                        <span className="font-semibold text-sm">
                            {authorName}
                        </span>
                        <span className="text-xs text-gray-400">
                            {DatetimeToShow(comment.created_at)}
                        </span>
                        {currentUserId === comment.author_id && (
                            <Button
                                size="small"
                                variant="danger"
                                onClick={() => onDelete(comment.id)}
                            >
                                Delete
                            </Button>
                        )}
                    </div>
                    {parentNickname && (
                        <div className="text-xs text-gray-400 mt-0.5">
                            Replying to{" "}
                            <span className="font-medium text-gray-500">
                                @{parentNickname}
                            </span>
                        </div>
                    )}
                </div>
            </div>

            {/* Body */}
            <div className="px-3 pt-1 pb-2">
                <div className="ml-[44px] prose max-w-none text-sm">
                    <MDEditor.Markdown source={comment.content} />
                </div>
            </div>

            {/* Actions */}
            <div className="ml-[44px] px-3 pb-3 flex items-center gap-3 border-t border-gray-100 pt-2">
                <Button size="small" onClick={() => setShowReply(!showReply)}>
                    Reply
                </Button>
                {comment.replies.length > 0 && (
                    <span className="text-xs text-gray-400">
                        {comment.replies.length}{" "}
                        {comment.replies.length === 1 ? "reply" : "replies"}
                    </span>
                )}
            </div>

            {/* Reply form */}
            {showReply && (
                <div className="ml-[44px] px-3 pb-3">
                    <CommentEditor
                        value={replyContent}
                        setValue={setReplyContent}
                        avatar={meAvatar}
                        nickname={meNickname}
                        onSubmit={submitReply}
                        onCancel={() => setShowReply(false)}
                    />
                </div>
            )}

            {/* Threaded replies */}
            {comment.replies.length > 0 && (
                <div className="ml-[44px] border-l-2 border-gray-100 pl-4 pr-3 pb-3 space-y-2">
                    {comment.replies.map((reply) => (
                        <CommentItem
                            key={reply.id}
                            comment={reply}
                            discussionId={discussionId}
                            currentUserId={currentUserId}
                            meAvatar={meAvatar}
                            meNickname={meNickname}
                            parentNickname={authorName}
                            onReply={onReply}
                            onDelete={onDelete}
                        />
                    ))}
                </div>
            )}
        </div>
    );
}

function RouteComponent() {
    const params = Route.useParams();
    const id = params.id!;
    const queryClient = useQueryClient();
    const banner = useMsgBanner();

    const { data, isLoading, isError } = useQuery({
        queryKey: ["discussion", id],
        queryFn: () => serviceApi.discussions.get(id),
    });

    const discussion = data?.data;

    const { data: meData } = useQuery({
        queryKey: ["profile"],
        queryFn: () => serviceApi.users.getMe(),
        select: (res) => res.data,
    });
    const currentUserId = meData?.id;
    const meNickname = meData?.nickname || "";
    const meAvatar = meData?.avatar;
    const isAuthor = currentUserId === discussion?.author_id;

    const [editContent, setEditContent] = useState(discussion?.content || "");
    const [editorKey, setEditorKey] = useState(0);
    useEffect(() => {
        if (discussion?.content != null) {
            setEditContent(discussion.content);
            setEditorKey((k) => k + 1);
        }
    }, [discussion?.content]);

    const patchMutation = useMutation({
        mutationFn: (data: Partial<Discussions>) =>
            serviceApi.discussions.patch(data),
        onSuccess: () => {
            queryClient.invalidateQueries({ queryKey: ["discussion", id] });
            banner.showBanner("success", "Discussion updated");
        },
        onError: () => {
            banner.showBanner("critical", "Failed to update discussion");
        },
    });

    const { data: commentsData, refetch: refetchComments } = useQuery({
        queryKey: ["discussion-comments", id],
        queryFn: () => serviceApi.discussions.getComments(id),
    });

    const comments = commentsData?.data || [];

    const likeMutation = useMutation({
        mutationFn: () => serviceApi.discussions.like(id),
        onSuccess: () => {
            queryClient.invalidateQueries({ queryKey: ["discussion", id] });
        },
        onError: () => {
            banner.showBanner("critical", "Failed to like");
        },
    });

    const unlikeMutation = useMutation({
        mutationFn: () => serviceApi.discussions.unlike(id),
        onSuccess: () => {
            queryClient.invalidateQueries({ queryKey: ["discussion", id] });
        },
        onError: () => {
            banner.showBanner("critical", "Failed to unlike");
        },
    });

    const createCommentMutation = useMutation({
        mutationFn: (data: { content: string; parent_id?: string }) =>
            serviceApi.discussions.createComment(id, data),
        onSuccess: () => {
            refetchComments();
            queryClient.invalidateQueries({ queryKey: ["discussion", id] });
        },
        onError: () => {
            banner.showBanner("critical", "Failed to post comment");
        },
    });

    const deleteCommentMutation = useMutation({
        mutationFn: (comment_id: string) =>
            serviceApi.discussions.deleteComment(id, comment_id),
        onSuccess: () => {
            refetchComments();
            queryClient.invalidateQueries({ queryKey: ["discussion", id] });
        },
        onError: () => {
            banner.showBanner("critical", "Failed to delete comment");
        },
    });

    useTitle(`${discussion?.title ?? "Discussion"} | FloatCTF`);

    const [newComment, setNewComment] = useState("");

    const handlePostComment = () => {
        if (!newComment.trim()) {
            banner.showBanner("critical", "Comment cannot be empty");
            return;
        }
        createCommentMutation.mutate({ content: newComment });
        setNewComment("");
    };

    const handleReply = (parentId: string, content: string) => {
        createCommentMutation.mutate({ content, parent_id: parentId });
    };

    const handleDeleteComment = (commentId: string) => {
        deleteCommentMutation.mutate(commentId);
    };

    const handleLike = () => {
        likeMutation.mutate();
    };

    const handleUnlike = () => {
        unlikeMutation.mutate();
    };

    if (isLoading) {
        return <div className="p-8">Loading...</div>;
    }

    if (isError || !discussion) {
        return (
            <div className="p-8 text-red-500">
                Discussion not found or failed to load.
            </div>
        );
    }

    const commentTree = buildCommentTree(comments);

    return (
        <div className="h-full flex flex-col">
            <div className="flex-1 overflow-auto">
                <div className="flex flex-col pt-3 px-8 gap-3">
                    <h2 className="text-xl font-bold">{discussion.title}</h2>

                    <div className="flex items-center gap-3">
                        {discussion.author_avatar ? (
                            <Avatar src={discussion.author_avatar} size={32} />
                        ) : (
                            <div
                                className="shrink-0 flex items-center justify-center rounded-full bg-gray-200 text-gray-500 font-medium"
                                style={{ width: 32, height: 32, fontSize: 14 }}
                            >
                                {discussion.author_nickname?.[0]?.toUpperCase() ||
                                    "?"}
                            </div>
                        )}
                        <div>
                            <span className="font-medium">
                                {discussion.author_nickname}
                            </span>
                            <span className="text-gray-500 text-sm ml-2">
                                {DatetimeToShow(discussion.created_at)}
                            </span>
                        </div>
                    </div>

                    <div className="flex gap-4 text-sm text-gray-600">
                        <span className="flex items-center gap-1">
                            <EyeIcon size={16} />
                            {discussion.view_count} views
                        </span>
                        <span
                            className="flex items-center gap-1 cursor-pointer select-none"
                            onClick={
                                discussion.is_liked ? handleUnlike : handleLike
                            }
                        >
                            <ThumbsupIcon
                                size={16}
                                className={
                                    discussion.is_liked ? "text-[#1a7f37]" : ""
                                }
                            />
                            {discussion.like_count} likes
                        </span>
                        <span className="flex items-center gap-1">
                            <CommentIcon size={16} />
                            {discussion.comment_count} comments
                        </span>
                    </div>

                    <div className="border-top my-2" />

                    {isAuthor ? <banner.BannerComponent /> : null}
                    {isAuthor ? (
                        <div
                            key={editorKey}
                            className="min-h-[400px] flex flex-col"
                        >
                            <MDPlusEditor
                                className="flex-1 min-h-0"
                                height={400}
                                value={editContent}
                                setValue={setEditContent}
                                onSave={() => {
                                    patchMutation.mutate({
                                        id,
                                        content: editContent,
                                    });
                                }}
                            />
                        </div>
                    ) : (
                        <div className="prose max-w-none">
                            <MDEditor.Markdown source={discussion.content} />
                        </div>
                    )}

                    <div className="border-top my-4" />

                    <h3 className="text-lg font-semibold">
                        Comments ({comments.length})
                    </h3>

                    <div className="mb-4">
                        <CommentEditor
                            value={newComment}
                            setValue={setNewComment}
                            avatar={meAvatar}
                            nickname={meNickname}
                            onSubmit={handlePostComment}
                        />
                    </div>

                    <div className="flex flex-col gap-3">
                        {commentTree.map((comment) => (
                            <CommentItem
                                key={comment.id}
                                comment={comment}
                                discussionId={id}
                                currentUserId={currentUserId}
                                meAvatar={meAvatar}
                                meNickname={meNickname}
                                onReply={handleReply}
                                onDelete={handleDeleteComment}
                            />
                        ))}
                    </div>
                </div>
            </div>
        </div>
    );
}
