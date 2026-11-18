--TEST--
PHP URL splitting and form encoding
--FILE--
<?php
foreach (['http://u:p@host:80/p?#', '/a:2', 'file:///c:/a', 'http://[::1]:0', "\0?\xff"] as $url) {
    var_export(parse_url($url));
    echo "\n";
}
var_dump(parse_url('/api?debug', PHP_URL_PATH), parse_url('/api?', PHP_URL_QUERY));
parse_str('a.b=1&a[]=2&a[5]=3&a[]=4&empty&bin=%00%FF', $query);
echo http_build_query($query, '', '&', PHP_QUERY_RFC3986), "\n";
--EXPECT--
array (
  'scheme' => 'http',
  'host' => 'host',
  'port' => 80,
  'user' => 'u',
  'pass' => 'p',
  'path' => '/p',
  'query' => '',
  'fragment' => '',
)
false
array (
  'scheme' => 'file',
  'path' => 'c:/a',
)
array (
  'scheme' => 'http',
  'host' => '[::1]',
  'port' => 0,
)
array (
  'path' => '_',
  'query' => 'ÿ',
)
string(4) "/api"
string(0) ""
a_b=1&a%5B0%5D=2&a%5B5%5D=3&a%5B6%5D=4&empty=&bin=%00%FF
