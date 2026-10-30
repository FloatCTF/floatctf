import { Checkbox, CheckboxGroup, FormControl, Textarea, TextInput } from "@primer/react";

/** 应用表单的可编辑字段（创建与编辑共用）。回调地址以多行文本编辑，每行一个。 */
export type OidcClientFormState = {
	name: string;
	homepage_url: string;
	description: string;
	redirect_uris_text: string;
	scopes: string[];
	/** 标准退出（编辑页提供；创建表单不带时服务端保持为空）。 */
	post_logout_redirect_uris_text?: string;
	backchannel_logout_uri?: string;
};

export const emptyOidcClientForm = (): OidcClientFormState => ({
	name: "",
	homepage_url: "",
	description: "",
	redirect_uris_text: "",
	scopes: ["openid", "profile"],
});

export const redirectUrisOf = (text: string) =>
	text
		.split("\n")
		.map((line) => line.trim())
		.filter(Boolean);

/** 表单 → SDK 输入。 */
export const toOidcClientInput = (form: OidcClientFormState) => ({
	name: form.name.trim(),
	homepage_url: form.homepage_url.trim() || null,
	description: form.description.trim() || null,
	redirect_uris: redirectUrisOf(form.redirect_uris_text),
	scopes: Array.from(new Set(["openid", ...form.scopes])),
	...(form.post_logout_redirect_uris_text === undefined
		? {}
		: { post_logout_redirect_uris: redirectUrisOf(form.post_logout_redirect_uris_text) }),
	...(form.backchannel_logout_uri === undefined
		? {}
		: { backchannel_logout_uri: form.backchannel_logout_uri.trim() }),
});

/** 标准退出：退出后回跳地址（每行一个）与后端退出通知地址。 */
export const LogoutUrisInputs = {
	PostLogout: ({
		form,
	}: {
		form: Pick<OidcClientFormState, "post_logout_redirect_uris_text">;
	}) => (
		<Textarea
			value={form.post_logout_redirect_uris_text ?? ""}
			onChange={(e) => {
				form.post_logout_redirect_uris_text = e.target.value;
			}}
			rows={2}
			placeholder="https://app.example.com/auth/signed-out"
			className="font-mono"
		/>
	),
	Backchannel: ({ form }: { form: Pick<OidcClientFormState, "backchannel_logout_uri"> }) => (
		<TextInput
			value={form.backchannel_logout_uri ?? ""}
			onChange={(e) => {
				form.backchannel_logout_uri = e.target.value;
			}}
			placeholder="https://app.example.com/api/oidc/backchannel-logout"
			className="font-mono"
		/>
	),
};

export const RedirectUrisInput = ({
	form,
}: {
	form: Pick<OidcClientFormState, "redirect_uris_text">;
}) => (
	<Textarea
		value={form.redirect_uris_text}
		onChange={(e) => {
			form.redirect_uris_text = e.target.value;
		}}
		rows={3}
		placeholder="https://app.example.com/auth/callback"
		className="font-mono"
	/>
);

export const ScopesInput = ({ form }: { form: Pick<OidcClientFormState, "scopes"> }) => (
	<CheckboxGroup>
		<CheckboxGroup.Label visuallyHidden>Scopes</CheckboxGroup.Label>
		{[
			{ value: "openid", label: "openid — 账号标识（必需）", disabled: true },
			{ value: "profile", label: "profile — 用户名、昵称、头像" },
			{ value: "email", label: "email — 邮箱（email_verified 恒为 false）" },
		].map((option) => (
			<FormControl key={option.value} disabled={option.disabled}>
				<Checkbox
					value={option.value}
					checked={option.value === "openid" || form.scopes.includes(option.value)}
					onChange={(e) => {
						form.scopes = e.target.checked
							? Array.from(new Set([...form.scopes, option.value]))
							: form.scopes.filter((s) => s !== option.value);
					}}
				/>
				<FormControl.Label>{option.label}</FormControl.Label>
			</FormControl>
		))}
	</CheckboxGroup>
);

/** 名称 / 主页 / 简介（创建与编辑共用的文本字段）。 */
export const ClientTextInputs = {
	Name: ({ form }: { form: Pick<OidcClientFormState, "name"> }) => (
		<TextInput
			value={form.name}
			maxLength={64}
			onChange={(e) => {
				form.name = e.target.value;
			}}
			placeholder="例如 LearnOS"
		/>
	),
	Homepage: ({ form }: { form: Pick<OidcClientFormState, "homepage_url"> }) => (
		<TextInput
			value={form.homepage_url}
			onChange={(e) => {
				form.homepage_url = e.target.value;
			}}
			placeholder="https://app.example.com"
		/>
	),
	Description: ({ form }: { form: Pick<OidcClientFormState, "description"> }) => (
		<Textarea
			value={form.description}
			maxLength={500}
			onChange={(e) => {
				form.description = e.target.value;
			}}
			rows={2}
		/>
	),
};
