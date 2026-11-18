--TEST--
Query encoding includes public object properties in order
--FILE--
<?php
class QueryData {
    public array $filters = ['published' => true, 'owner' => null];
    public string $query = 'chat & chien';
    private int $secret = 99;
    protected string $internal = 'keep private';
}
echo http_build_query(new QueryData, 'key_', ' | ', PHP_QUERY_RFC3986), "\n";
echo http_build_query((object) [3 => 'present'], 'key_'), "\n";
--EXPECT--
filters%5Bpublished%5D=1 | query=chat%20%26%20chien
3=present
