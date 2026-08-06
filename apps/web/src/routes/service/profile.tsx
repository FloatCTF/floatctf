import { PencilIcon } from "@primer/octicons-react";
import { Avatar, Button, FormControl, TextInput } from "@primer/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useReactive } from "ahooks";
import type { AxiosError } from "axios";
import { useEffect, useRef, useState } from "react";

import { serviceApi } from "@/api";
import { useMsgBanner } from "@/components";
import type { Users } from "@/entity";
import { diffToPatch } from "@/util";

export const Route = createFileRoute("/service/profile")({
    component: RouteComponent,
});

function RouteComponent() {
    const queryClient = useQueryClient();
    const [editable, setEditable] = useState(false);
    const { data, isLoading } = useQuery({
        queryKey: ["profile"],
        queryFn: () => serviceApi.users.getMe(),
        select: (res) => res.data,
    });

    const [originalProfile, setOriginalProfile] = useState<Partial<Users>>({});
    const banner = useMsgBanner();

    // Avatar upload state
    const [pendingFile, setPendingFile] = useState<File | null>(null);
    const [previewUrl, setPreviewUrl] = useState<string>("");
    const fileInputRef = useRef<HTMLInputElement>(null);

    const mutationProfile = useReactive<Partial<Users>>({
        username: "",
        nickname: "",
        email: "",
        password: "",
    });

    const patchMutation = useMutation({
        mutationFn: serviceApi.users.patchMe,
        onSuccess: () => {
            setEditable(false);
            banner.showBanner("success", "Update profile successfully!");
        },
        onError: (e: AxiosError<{ msg: string }>) => {
            if (e.response?.data) {
                banner.showBanner("critical", e.response?.data.msg);
            }
        },
    });

    const avatarUploadMutation = useMutation({
        mutationFn: serviceApi.uploads.upload_avatar,
        onSuccess: () => {
            setPendingFile(null);
            setPreviewUrl("");
            queryClient.invalidateQueries({ queryKey: ["profile"] });
            banner.showBanner("success", "Avatar updated successfully!");
        },
        onError: () => {
            banner.showBanner("critical", "Failed to upload avatar");
        },
    });

    const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
        const file = e.target.files?.[0];
        if (!file) return;

        setPendingFile(file);
        setPreviewUrl(URL.createObjectURL(file));

        // Clear input so selecting the same file again triggers onChange
        e.target.value = "";
    };

    const handleUploadAvatar = () => {
        if (pendingFile) {
            avatarUploadMutation.mutate(pendingFile);
        }
    };

    const handleCancelPreview = () => {
        setPendingFile(null);
        URL.revokeObjectURL(previewUrl);
        setPreviewUrl("");
    };

    // Cleanup object URL on unmount
    useEffect(() => {
        return () => {
            if (previewUrl) URL.revokeObjectURL(previewUrl);
        };
    }, [previewUrl]);

    useEffect(() => {
        if (data) {
            Object.assign(mutationProfile, data);
            setOriginalProfile(data);
        }
    }, [data, mutationProfile]);

    if (isLoading) {
        return <div>Loading...</div>;
    }

    // Determine the avatar source: pending preview > server avatar
    const avatarSrc = previewUrl || data?.avatar || "";

    return (
        <div className="px-3 w-full">
            <h3 className="border-bottom py-2 my-3">Profile</h3>

            {/* Avatar Section */}
            <div className="flex flex-col items-center mb-6 gap-2">
                <div
                    className="relative group cursor-pointer inline-block rounded-full"
                    onClick={() => fileInputRef.current?.click()}
                >
                    {avatarSrc ? (
                        <Avatar src={avatarSrc} size={120} alt="Avatar" />
                    ) : (
                        <div className="w-20 h-20 rounded-full bg-gray-200 flex  border border-gray-300">
                            <span className="text-2xl font-medium text-gray-500">
                                {data?.nickname?.[0]?.toUpperCase() || "?"}
                            </span>
                        </div>
                    )}

                    {/* Hover overlay */}
                    <div className="absolute inset-0 bg-black/50 rounded-full opacity-0 group-hover:opacity-100 flex items-center justify-center transition-opacity z-10">
                        <span className="text-white text-sm font-medium">
                            Select
                        </span>
                    </div>
                </div>

                {/* Hidden file input */}
                <input
                    ref={fileInputRef}
                    type="file"
                    accept="image/*"
                    className="hidden"
                    onChange={handleFileChange}
                />

                {/* Upload / Cancel buttons (shown only when a new file is selected) */}
                {pendingFile && (
                    <div className="flex gap-2">
                        <Button
                            size="small"
                            variant="primary"
                            onClick={handleUploadAvatar}
                            disabled={avatarUploadMutation.isPending}
                        >
                            {avatarUploadMutation.isPending
                                ? "Uploading..."
                                : "Upload"}
                        </Button>
                        <Button
                            size="small"
                            variant="danger"
                            onClick={handleCancelPreview}
                        >
                            Cancel
                        </Button>
                    </div>
                )}
            </div>

            <div className="flex flex-col gap-2">
                <FormControl>
                    <FormControl.Label>Username</FormControl.Label>
                    <TextInput
                        value={mutationProfile.username}
                        disabled={true}
                        onChange={(e) => {
                            mutationProfile.username = e.target.value;
                        }}
                    />
                    <FormControl.Caption>
                        This is can not mutate, or contact the administrator
                    </FormControl.Caption>
                </FormControl>
                <FormControl>
                    <FormControl.Label>Nickname</FormControl.Label>
                    <TextInput
                        value={mutationProfile.nickname}
                        disabled={!editable}
                        onChange={(e) => {
                            mutationProfile.nickname = e.target.value;
                        }}
                    />
                    <FormControl.Caption>
                        The nickname will be displayed in the public area
                    </FormControl.Caption>
                </FormControl>
                <FormControl>
                    <FormControl.Label>Email</FormControl.Label>
                    <TextInput
                        value={mutationProfile.email}
                        disabled={!editable}
                        onChange={(e) => {
                            mutationProfile.email = e.target.value;
                        }}
                    />
                </FormControl>
                <FormControl>
                    <FormControl.Label>Password</FormControl.Label>
                    <TextInput
                        value={mutationProfile.password}
                        disabled={!editable}
                        onChange={(e) => {
                            mutationProfile.password = e.target.value;
                        }}
                    />
                    <FormControl.Caption>Fill it</FormControl.Caption>
                </FormControl>
                {!editable && (
                    <Button className="w-fit" onClick={() => setEditable(true)}>
                        <PencilIcon />
                        &emsp;Edit
                    </Button>
                )}
                {editable && (
                    <div className="flex gap-2">
                        <Button
                            variant="danger"
                            onClick={() => setEditable(false)}
                        >
                            Cancel
                        </Button>
                        <Button
                            variant="primary"
                            onClick={() => {
                                const payload = diffToPatch(
                                    originalProfile,
                                    mutationProfile,
                                );
                                patchMutation.mutate(payload);
                            }}
                        >
                            Save
                        </Button>
                    </div>
                )}
                <banner.BannerComponent />
            </div>
        </div>
    );
}
