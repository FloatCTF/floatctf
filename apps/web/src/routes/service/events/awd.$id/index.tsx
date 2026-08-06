import { Button, FormControl, TextInput } from "@primer/react";
import { useMutation } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";

import { serviceApi } from "@/api";
import { useMsgBanner } from "@/components";
import { ServiceRouteGuard } from "../../route";

export const Route = createFileRoute("/service/events/awd/$id/")({
	component: RouteComponent,
	loader: ServiceRouteGuard,
});

function RouteComponent() {
	const { id } = Route.useParams();
	const banner = useMsgBanner({});
	const [flag, setFlag] = useState("");

	const submit = useMutation({
		mutationFn: () => serviceApi.awd.submitFlag(id, flag),
		onSuccess: (res) => {
			if (res.code === 0) {
				banner.showBanner("success", "Flag accepted");
				setFlag("");
			} else {
				banner.showBanner("critical", res.message || "Submit failed");
			}
		},
		onError: (e: Error) => banner.showErrorBanner(e),
	});

	return (
		<div className="flex flex-col gap-3 max-w-xl">
			<banner.BannerComponent />
			<p>
				AWD attack/defense event. Open GameBoxes for targets, WireGuard for VPN
				access, and Scoreboard for ranking.
			</p>
			<FormControl>
				<FormControl.Label>Submit Flag</FormControl.Label>
				<TextInput
					value={flag}
					onChange={(e) => setFlag(e.target.value)}
					placeholder="flag{...}"
					block
				/>
			</FormControl>
			<Button
				variant="primary"
				disabled={!flag || submit.isPending}
				onClick={() => submit.mutate()}
			>
				Submit
			</Button>
		</div>
	);
}
