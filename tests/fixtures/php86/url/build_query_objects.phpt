--TEST--
Query encoding includes public object properties in order
--FILE--
<?php
set_error_handler(function ($severity, $message) { echo $severity, ": ", $message, PHP_EOL; });
class QueryData {
    public array $filters = ['published' => true, 'owner' => null];
    public string $query = 'chat & chien';
    private int $secret = 99;
    protected string $internal = 'keep private';
}
echo http_build_query(new QueryData, 'key_', ' | ', PHP_QUERY_RFC3986), "\n";
echo http_build_query((object) [3 => 'present'], 'key_'), "\n";
$nested = (object) ['x' => 1];
echo http_build_query(['a' => [$nested], 'b' => $nested]), "\n";
--EXPECT--
8192: http_build_query(): Passing an object for argument #1 $data to http_build_query() is deprecated, call get_object_vars() first instead
filters%5Bpublished%5D=1 | query=chat%20%26%20chien
8192: http_build_query(): Passing an object for argument #1 $data to http_build_query() is deprecated, call get_object_vars() first instead
3=present
8192: http_build_query(): object values within argument #1 $data to http_build_query() being interpreted as arrays is deprecated, instead the $data argument should be preprocessed with get_object_vars()
8192: http_build_query(): object values within argument #1 $data to http_build_query() being interpreted as arrays is deprecated, instead the $data argument should be preprocessed with get_object_vars()
a%5B0%5D%5Bx%5D=1&b%5Bx%5D=1
