--TEST--
The built-ins as conditions, in global code and in a namespace, with the namespace's own function where one is declared.
--FILE--
<?php
// The built-ins as conditions, in global code and in a namespace, with the
// namespace's own function where one is declared.
namespace Plain {
    $values = [null, 0, '', [], 'x'];
    foreach ($values as $value) {
        $seen = [];
        if (is_null($value)) { $seen[] = 'null'; }
        if (!is_null($value)) { $seen[] = 'not null'; }
        if (is_null($value) && is_string($value)) { $seen[] = 'both'; }
        if (is_array($value) || is_null($value)) { $seen[] = 'array or null'; }
        while (is_null($value)) { $seen[] = 'loop'; $value = 1; }
        echo json_encode($seen), "\n";
    }
}
namespace Mine {
    function is_null($value) { return $value === 'x' ? 0 : 'yes'; }
    foreach ([null, 'x', 1] as $value) {
        $seen = [];
        if (is_null($value)) { $seen[] = 'mine true'; }
        if (!is_null($value)) { $seen[] = 'mine false'; }
        echo json_encode($seen), "\n";
    }
}
--EXPECT--
["null","array or null","loop"]
["not null"]
["not null"]
["not null","array or null"]
["not null"]
["mine true"]
["mine false"]
["mine true"]
