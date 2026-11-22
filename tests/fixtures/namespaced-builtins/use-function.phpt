--TEST--
`use function` names one function; no guard applies.
--FILE--
<?php
// `use function` names one function; no guard applies.
namespace Other {
    function is_null($value) { return 'Other\is_null'; }
}
namespace App {
    function is_null($value) { return 'App\is_null'; }
}
namespace App\Global_ {
    use function is_null;
    function is_array($value) { return 'App\Global_\is_array'; }
    for ($round = 0; $round < 2; $round++) { var_dump(is_null(null), \is_array([])); }
}
namespace App\Imported {
    use function Other\is_null;
    for ($round = 0; $round < 2; $round++) { var_dump(is_null(null)); }
}
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
string(13) "Other\is_null"
string(13) "Other\is_null"
