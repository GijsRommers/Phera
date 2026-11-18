<?php
// Rule: Phera omits built-ins and resolves project declarations in their place.
namespace {
    function lcg_value() {}
    const MT_RAND_PHP = 1;
}
namespace App {
    function strftime() {}
    class Redis { public function sortAsc($name) {} }
    const FILTER_SANITIZE_STRING = 3;
    lcg_value();
    strftime();
    (new Redis)->sortAsc('key');
    echo MT_RAND_PHP, FILTER_SANITIZE_STRING;
}
