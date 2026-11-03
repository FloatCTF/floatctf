/** 404 —— 真实路由之外的任何路径。 */
import { useNavigate } from "react-router";
import { Button, Card } from "animal-island-ui";
import { CompassIcon } from "naive-icons";

export function NotFound() {
	const navigate = useNavigate();
	return (
		<div style={{ padding: 24 }}>
			<Card>
				<div className="ai-state">
					<div className="ai-state-icon">
						<CompassIcon size={28} />
					</div>
					<div className="ai-state-title">这里没有路</div>
					<div className="ai-muted">你访问的页面不存在，或者已经被移到别处。</div>
					<Button type="primary" onClick={() => void navigate("/island")}>
						回到岛屿
					</Button>
				</div>
			</Card>
		</div>
	);
}
