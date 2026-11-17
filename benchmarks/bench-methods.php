<?php
class BaseCall { public function parentCall() { return 1; } }
class MixedCall extends BaseCall {
    public function instanceCall() { return self::staticCall() + parent::parentCall(); }
    public static function staticCall() { return 2; }
}
$object = new MixedCall;
$sum = 0;
for ($i = 0; $i < 5000000; $i++) {
    $sum += $object->instanceCall() + MixedCall::staticCall();
}
echo $sum, "\n";
