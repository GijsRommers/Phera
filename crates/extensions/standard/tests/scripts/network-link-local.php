<?php
// Compare both runtimes on the same host. The addresses are system-provided.
$addresses = [];
foreach (net_get_interfaces() as $interface) {
    foreach ($interface['unicast'] as $entry) {
        $address = inet_pton($entry['address'] ?? '');
        if ($address !== false && strlen($address) === 16
            && ord($address[0]) === 0xfe && (ord($address[1]) & 0xc0) === 0x80) {
            $addresses[] = bin2hex($address);
        }
    }
}
sort($addresses, SORT_STRING);
echo json_encode($addresses), "\n";
