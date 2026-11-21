<?php
$listener = stream_socket_server('tcp://127.0.0.1:0');
$address = stream_socket_get_name($listener, false);
$flags = STREAM_CLIENT_CONNECT | STREAM_CLIENT_PERSISTENT;

foreach ([
    fn() => pfsockopen($address),
    fn() => stream_socket_client($address, flags: $flags),
] as $open) {
    $first = $open();
    $second = $open();
    $peer = stream_socket_accept($listener, 0.5);
    var_dump($first === $second);
    $before = stream_socket_get_name($first, false);
    fclose($first);
    fclose($peer);
    var_dump(is_resource($second));
    $again = $open();
    var_dump($before !== stream_socket_get_name($again, false));
    fclose($again);
    fclose(stream_socket_accept($listener, 0.5));
}

$source = stream_socket_server('tcp://127.0.0.1:0');
$name = stream_socket_get_name($source, false);
fclose($source);
$context = stream_context_create(['socket' => ['bindto' => $name]]);
$client = stream_socket_client($address, context: $context);
var_dump($name === stream_socket_get_name($client, false));
$accepted = stream_socket_accept($listener, 0.5);
var_dump($name === stream_socket_get_name($accepted, true));
fclose($client);
fclose($accepted);

set_error_handler(function ($level, $message) {
    echo preg_replace('~127\.0\.0\.1:\d+~', '<address>', $message), "\n";
});
foreach ([false, 'localhost:0', $address] as $binding) {
    $context = stream_context_create(['socket' => ['bindto' => $binding]]);
    $client = stream_socket_client($address, $error, $message, 0.5, context: $context);
    var_dump(is_resource($client), $error, $message);
    if ($client) {
        fclose($client);
        fclose(stream_socket_accept($listener, 0.5));
    }
}
restore_error_handler();
