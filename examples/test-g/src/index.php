<?php
// 获取用户输入的 URL
$url = $_GET['url'];
if (isset($url)) {
    // 1. 初始化 curl
    $ch = curl_init();

    // 2. 设置配置
    curl_setopt($ch, CURLOPT_URL, $url);           // 设置目标 URL
    curl_setopt($ch, CURLOPT_HEADER, 0);           // 不返回 header
    curl_setopt($ch, CURLOPT_RETURNTRANSFER, 1);   // 将结果返回成字符串而非直接输出

    // 3. 执行请求
    $result = curl_exec($ch);

    // 4. 关闭连接并输出结果
    curl_close($ch);
    echo $result;
} else {
    echo "Please usage: ?url=http://cn.bing.com";
}
?>
