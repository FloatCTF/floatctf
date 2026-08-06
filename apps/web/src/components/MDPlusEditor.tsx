import { uploadsServiceApi } from "@/api/service";
import { ImageIcon, UploadIcon } from "@primer/octicons-react";
import { Button } from "@primer/react";
import MDEditor, {
    codeEdit,
    codeLive,
    codePreview,
    type ICommand,
} from "@uiw/react-md-editor";
import mermaid from "mermaid";
import {
    Fragment,
    type ReactNode,
    useCallback,
    useEffect,
    useRef,
    useState,
} from "react";
import { getCodeString } from "rehype-rewrite";

const randomid = () =>
    Number.parseInt(String(Math.random() * 1e15), 10).toString(36);

interface CodeProps {
    inline?: boolean;
    children?: ReactNode;
    className?: string;
    // biome-ignore lint/suspicious/noExplicitAny: <explanation>
    node?: any;
}

const Code = ({ inline, children = [], className, ...props }: CodeProps) => {
    const demoid = useRef(`dome${randomid()}`);
    const [container, setContainer] = useState<HTMLElement | null>(null);
    const isMermaid =
        className && /^language-mermaid/.test(className.toLocaleLowerCase());
    const code = props?.node?.children
        ? getCodeString(props.node.children)
        : Array.isArray(children) && children.length > 0
          ? (children[0] as string) || ""
          : "";

    useEffect(() => {
        if (container && isMermaid && demoid.current && code) {
            mermaid
                .render(demoid.current, code)
                .then(({ svg, bindFunctions }) => {
                    container.innerHTML = svg;
                    if (bindFunctions) {
                        bindFunctions(container);
                    }
                })
                .catch((error) => {
                    console.error("Mermaid 渲染错误:", error);
                });
        }
    }, [container, isMermaid, code]);

    const refElement = useCallback((node: HTMLElement | null) => {
        if (node !== null) {
            setContainer(node);
        }
    }, []);

    if (isMermaid) {
        return (
            <Fragment>
                <code id={demoid.current} style={{ display: "none" }} />
                <code
                    className={className}
                    ref={refElement}
                    data-name="mermaid"
                />
            </Fragment>
        );
    }
    return <code className={className}>{children}</code>;
};

export interface MDPlusEditorProps {
    value: string;
    setValue: (value: string) => void;
    className?: string;
    height?: any;
    onSave?: () => void;
}

export const MDPlusEditor = ({
    value,
    setValue,
    className,
    height,
    onSave,
}: MDPlusEditorProps) => {
    const editorRef = useRef<HTMLTextAreaElement | null>(null);
    const save: ICommand = {
        name: "Save",
        keyCommand: "Save",
        buttonProps: { "aria-label": "Save doc" },
        icon: <UploadIcon />,
        execute: () => {
            onSave?.();
        },
    };
    const blur: ICommand = {
        name: "Blur",
        keyCommand: "Blur",
        buttonProps: { "aria-label": "Blur Box" },
        icon: (
            <svg
                xmlns="http://www.w3.org/2000/svg"
                width="1em"
                height="1em"
                viewBox="0 0 24 24"
            >
                <title>Blur</title>
                <path
                    fill="currentColor"
                    d="M11 6a1 1 0 0 1 1-1h9a1 1 0 1 1 0 2h-9a1 1 0 0 1-1-1m-9 4a1 1 0 0 1 1-1h11a1 1 0 1 1 0 2H3a1 1 0 0 1-1-1m3 3a1 1 0 1 0 0 2h9a1 1 0 1 0 0-2zm12 0a1 1 0 1 0 0 2h4a1 1 0 1 0 0-2zm0-4a1 1 0 1 0 0 2h2a1 1 0 1 0 0-2zM7 18a1 1 0 0 1 1-1h11a1 1 0 1 1 0 2H8a1 1 0 0 1-1-1m-4-1a1 1 0 1 0 0 2h2a1 1 0 1 0 0-2zM5 5a1 1 0 0 0 0 2h4a1 1 0 0 0 0-2z"
                />
            </svg>
        ),
        execute: () => {
            if (!value.match("tip-box")) {
                setValue(
                    `<style>.tip-box{filter:blur(3px);transition:0.3s ease;display:inline-block}.tip-box:hover{filter:blur(0px)}</style>\n\n${value}<p class="tip-box">这里是被遮罩的敏感内容。  鼠标移上来以后就会变清晰。</p>\n\n`,
                );
            } else {
                setValue(
                    `${value}<p class="tip-box">这里是被遮罩的敏感内容。  鼠标移上来以后就会变清晰。</p>\n\n`,
                );
            }
        },
    };

    const tipBox: ICommand = {
        name: "TipBox",
        keyCommand: "TipBox",
        buttonProps: { "aria-label": "TipBox" },
        icon: (
            <svg
                xmlns="http://www.w3.org/2000/svg"
                width="1em"
                height="1em"
                viewBox="0 0 48 48"
            >
                <title>TipBox</title>
                <g
                    fill="none"
                    stroke="currentColor"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    strokeWidth="3"
                >
                    <path d="M13.707 38.823C16.485 38.926 19.9 39 24 39c7.371 0 12.525-.238 15.752-.463c2.55-.177 4.539-2.081 4.762-4.627C44.76 31.12 45 26.885 45 21s-.24-10.121-.486-12.91c-.223-2.546-2.212-4.45-4.762-4.627C36.525 3.238 31.372 3 24 3s-12.525.238-15.752.463c-2.55.177-4.539 2.081-4.762 4.627C3.24 10.88 3 15.115 3 21v22.652c0 1.708 2.004 2.63 3.302 1.518z" />
                    <path d="M27.882 26.952c2.218-1.238 3.36-2.588 3.538-4.88a96 96 0 0 1-2.028-.027c-1.33-.034-2.326-1.032-2.361-2.362a99 99 0 0 1-.031-2.61c0-1.16.015-2.069.035-2.77c.036-1.252.939-2.202 2.191-2.254C29.921 12.021 30.828 12 32 12s2.079.02 2.774.05c1.252.05 2.155 1.001 2.191 2.254c.02.695.035 1.594.035 2.74v5.029h-.023c-.296 4.236-3.425 6.76-6.97 7.85c-.499.154-1.05.08-1.427-.28c-.438-.419-.813-.937-1.088-1.368c-.295-.464-.09-1.055.39-1.323m-16 0c2.218-1.238 3.36-2.588 3.538-4.88a96 96 0 0 1-2.028-.027c-1.33-.034-2.326-1.032-2.361-2.362a99 99 0 0 1-.031-2.61c0-1.16.015-2.069.035-2.77c.036-1.252.939-2.202 2.191-2.254C13.921 12.021 14.828 12 16 12s2.079.02 2.774.05c1.252.05 2.155 1.001 2.191 2.254c.02.695.035 1.594.035 2.74v5.029h-.023c-.296 4.236-3.425 6.76-6.97 7.85c-.499.154-1.05.08-1.427-.28c-.438-.419-.813-.937-1.088-1.368c-.295-.464-.09-1.055.39-1.323" />
                </g>
            </svg>
        ),
        execute: () => {
            setValue(
                `${value}<details>
  <summary>点击查看内容</summary>
  <p>
    这里是被遮罩的敏感内容。
  </p>
</details>\n\n`,
            );
        },
    };

    const imageUpload: ICommand = {
        name: "ImageUpload",
        keyCommand: "ImageUpload",
        buttonProps: { "aria-label": "Upload Image" },
        icon: <ImageIcon />,
        execute: () => {
            const input = document.createElement("input");
            input.type = "file";
            input.accept = "image/*";
            input.onchange = async (e) => {
                const file = (e.target as HTMLInputElement).files?.[0];
                if (!file) return;
                try {
                    const url = await uploadsServiceApi.upload_image?.(file);
                    if (url.data) {
                        const result = url.data.startsWith(".")
                            ? url.data.slice(1)
                            : url.data;
                        const imageMarkdown = `${value}![${result}](${result})\n`;
                        setValue(imageMarkdown);
                    }
                } catch (err) {
                    console.error("图片上传失败:", err);
                }
            };
            input.click();
        },
    };

    const handlePaste = useCallback(
        async (event: ClipboardEvent) => {
            if (!event.clipboardData) return;

            const items = event.clipboardData.items;
            for (const item of items) {
                if (item.type.startsWith("image/")) {
                    event.preventDefault();
                    const file = item.getAsFile();
                    if (!file) return;

                    try {
                        const url =
                            await uploadsServiceApi.upload_image?.(file);

                        if (url.data) {
                            const result = url.data.startsWith(".")
                                ? url.data.slice(1)
                                : url.data;
                            const imageMarkdown = `${value}![${result}](${result})\n`;

                            setValue(imageMarkdown);
                        }
                    } catch (err) {
                        console.error("图片上传失败:", err);
                    }
                    break;
                }
            }
        },
        [setValue, value],
    );
    useEffect(() => {
        const el = editorRef.current;
        if (el) el.addEventListener("paste", handlePaste);
        return () => {
            if (el) el.removeEventListener("paste", handlePaste);
        };
    }, [handlePaste]);

    return (
        <MDEditor
            className={className}
            height={height}
            minHeight={300}
            value={value}
            onChange={(newValue = "") => setValue(newValue)}
            extraCommands={[
                imageUpload,
                tipBox,
                blur,
                save,
                codeEdit,
                codeLive,
                codePreview,
            ]}
            textareaProps={{
                // @ts-ignore
                onPaste: handlePaste,
                placeholder: "Write your idea down here...",
            }}
            previewOptions={{
                components: {
                    code: Code,
                },
            }}
        />
    );
};
