<?php
function ipv4Loopback($address): bool {
    return is_string($address) && str_starts_with($address, '127.')
        && inet_pton($address) !== false;
}

var_dump(ipv4Loopback(gethostbyname('localhost')));
var_dump(count(array_filter(gethostbynamel('localhost') ?: [], 'ipv4Loopback')) > 0);
foreach (['127.0.0.1', '192.0.2.91', '', '::1'] as $name) {
    var_dump(gethostbyname($name), gethostbynamel($name));
}

var_dump(getservbyname('http', 'tcp'), getservbyname('phera-absent', 'tcp'));
var_dump(getservbyport(80, 'tcp'), getservbyport(65616, 'tcp'));
var_dump(getprotobyname('tcp'), getprotobyname('phera-absent'));
var_dump(getprotobynumber(6), getprotobynumber(999));

$loopback = false;
foreach (net_get_interfaces() as $interface) {
    foreach ($interface['unicast'] as $address) {
        if (ipv4Loopback($address['address'] ?? false)) {
            $loopback = $interface['up'] && isset($address['netmask'])
                && is_int($address['flags']) && is_int($address['family']);
        }
    }
}
var_dump($loopback);

foreach ([
    fn() => gethostbyname("a\0b"),
    fn() => gethostbynamel("a\0b"),
    fn() => getservbyname("http\0", 'tcp'),
    fn() => getservbyport(80, "tcp\0"),
    fn() => getprotobyname("tcp\0"),
] as $call) {
    try {
        $call();
    } catch (Throwable $error) {
        echo $error::class, ' ', $error->getMessage(), "\n";
    }
}
