--TEST--
enums: imported enum
--FILE--
<?php
namespace Domain{enum E{case A;}} namespace Consumer{use Domain\E as Selected; var_dump(Selected::A);}
--EXPECT--
enum(Domain\E::A)
