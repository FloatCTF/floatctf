<?php
// FloatCTF AWDP GameBox 运行时契约：平台在启动实例时以 FLAG 环境变量注入该实例的 flag
// （见 awdp_practice_judge_settings.flag_path 默认值 /flag.php 及其注释：
//  "flag curl 验证的端点路径（如 /flag.php；GameBox 按 FLAG env 返回 flag）"）。
//
// 本文件是 examples/test-g 作为 **AWDP 测试夹具** 必须提供的端点：Judge /
// Break 流程通过 HTTP 读取它来验证实例可被攻破。真实比赛题目应当把 flag 藏在
// 漏洞之后（例如 index.php 的 SSRF 路径），而不是直接暴露这个端点。
$flag = getenv('FLAG');

if ($flag === false || $flag === '') {
    http_response_code(500);
    echo 'FLAG env not set';
    exit;
}

echo $flag;
