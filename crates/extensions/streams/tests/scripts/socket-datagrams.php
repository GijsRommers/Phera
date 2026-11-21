<?php
$socket = stream_socket_server('udp://127.0.0.1:0', flags: STREAM_SERVER_BIND);
stream_set_blocking($socket, false);
$sender = 'previous';
var_dump(stream_socket_recvfrom($socket, 16, address: $sender), $sender);
foreach ([$socket, false] as $source) {
    $sender = 'previous';
    try {
        stream_socket_recvfrom($source, 0, address: $sender);
    } catch (Throwable $error) {
        echo $error->getMessage(), "\n";
    }
    var_dump($sender);
}
set_error_handler(function ($level, $message) {
    var_dump($message);
});
var_dump(stream_socket_sendto($socket, 'ping', address: 'invalid'));
var_dump(stream_socket_sendto($socket, 'ping'));
restore_error_handler();
fclose($socket);
