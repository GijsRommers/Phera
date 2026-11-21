<?php

function show($value) {
    echo json_encode($value, JSON_THROW_ON_ERROR), "\n";
}

function notification() {}

$context = stream_context_create(
    ['http' => ['timeout' => 3]],
    ['notification' => 'notification', 'options' => ['x' => ['one' => 1]]]
);
show(stream_context_get_params($context));
show(stream_context_set_options($context, ['x' => ['two' => 2]]));
show(stream_context_set_params($context, ['options' => ['x' => ['three' => 3]]]));
show(stream_context_get_params($context));
foreach ([
    ['notification' => 'unknown_stream_notification'],
    ['notification' => ['UnknownStreamClass', 'notify']],
    ['options' => null],
] as $params) {
    try {
        stream_context_set_params($context, $params);
    } catch (TypeError $error) {
        show($error->getMessage());
        show(stream_context_get_params($context));
    }
}
show(stream_context_set_default(['http' => ['method' => 'HEAD']]) ===
    stream_context_get_default(['http' => ['timeout' => 2]]));
show(stream_context_get_options(stream_context_get_default()));

$file = fopen('php://temp', 'w+');
show(stream_context_get_params($file));
show(stream_context_set_options($file, ['custom' => ['value' => true]]));
show(stream_context_get_options($file));
show(set_file_buffer($file, 8));
fclose($file);

$url = $argv[1];
show(http_get_last_response_headers());
show(get_headers($url . '/ok', true));
show(http_get_last_response_headers());
show(get_headers($url . '/redirect', false, $context));
show(http_get_last_response_headers());
show(get_headers($url . '/missing', true));
show(@file_get_contents($url . '/missing', false, $context));
show(http_get_last_response_headers());
show(file_get_contents($url . '/redirect', false, $context));
show(http_get_last_response_headers());
$stream = fopen($url . '/ok', 'rb');
show(stream_context_get_options($stream));
show(stream_get_meta_data($stream)['wrapper_data']);
fclose($stream);
show(http_clear_last_response_headers());
show(http_get_last_response_headers());
