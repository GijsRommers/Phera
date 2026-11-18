--TEST--
enums: format export
--FILE--
<?php
namespace Domain; enum E{case A;} var_export(E::A);
--EXPECT--
\Domain\E::A
