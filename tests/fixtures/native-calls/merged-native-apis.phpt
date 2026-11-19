--TEST--
Compiled calls exercise the merged request, highlighting, OpenSSL and DOM APIs
--EXTENSIONS--
dom openssl
--FILE--
<?php
$key = openssl_pkey_new([
    'private_key_type' => OPENSSL_KEYTYPE_EC,
    'curve_name' => 'prime256v1',
]);
$details = openssl_pkey_get_details($key);
var_dump($details['bits']);
var_dump(openssl_sign('payload', $signature, $key, OPENSSL_ALGO_SHA256));
var_dump(openssl_verify('payload', $signature, $details['key'], OPENSSL_ALGO_SHA256));

$doc = new DOMDocument();
var_dump($doc->loadXML('<root>content</root>'));
var_dump($doc->getElementsByTagName('root')->item(0)->textContent);
var_dump(str_contains(highlight_file(__FILE__, true), '&lt;?php'));
try {
    request_parse_body();
} catch (RequestParseBodyException $error) {
    echo $error->getMessage(), "\n";
}
--EXPECT--
int(256)
bool(true)
int(1)
bool(true)
string(7) "content"
bool(true)
Request does not provide a content type
