--TEST--
Bound call storage releases receivers before object id reuse
--FILE--
<?php
class CallLifetime { function value(): int { return 3; } }
for ($i=0;$i<3;$i++) {
    $a=new CallLifetime;$value=$a->value();echo $value,':',spl_object_id($a),"\n";unset($a);
    $b=new CallLifetime;echo spl_object_id($b),"\n";unset($b);
}
--EXPECT--
3:1
1
3:1
1
3:1
1
