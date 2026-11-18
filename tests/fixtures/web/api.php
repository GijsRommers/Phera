<?php

$path = parse_url($_SERVER['REQUEST_URI'], PHP_URL_PATH);
parse_str($_SERVER['QUERY_STRING'] ?? '', $query);
$canonical = http_build_query($query);

header_remove('X-Powered-By');

header('Content-Type: application/json');
header_register_callback(function () {
    header_remove('X-Internal');
    header('X-Ready: yes');
});
header('X-Internal: temporary');
setcookie('seen', 'one two', 1, '/', '', false, true);
setcookie('theme', '', [
    'path' => '/',
    'secure' => true,
    'httponly' => true,
    'samesite' => 'Lax',
    'partitioned' => true,
]);

if ($path !== '/items') {
    http_response_code(404);
} elseif ($_SERVER['REQUEST_METHOD'] === 'POST') {
    http_response_code(201);
}

ob_start();
?>items template<?php
$template = ob_get_clean();

error_log('api ' . $path);
register_shutdown_function(fn () => print "\n");

echo json_encode([
    'path' => $path,
    'query' => $canonical,
    'form' => $_POST,
    'cookie' => $_COOKIE['client'] ?? null,
    'template' => $template,
    'headers' => headers_list(),
]);
