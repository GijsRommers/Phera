--TEST--
enums: namespaced enum
--FILE--
<?php
namespace Domain; enum E{case A;} echo get_class(E::A);
--EXPECT--
Domain\E
