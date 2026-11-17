<?php
interface Link { public function next(Link $value): Link; }
class FirstLink implements Link { public function next(Link $value): Link { return $value; } }
class SecondLink extends FirstLink { public function next(Link $value): Link { return $value; } }
class ThirdLink extends SecondLink { public function next(Link $value): Link { return $value; } }
$a = new ThirdLink;
$b = new ThirdLink;
for ($i = 0; $i < 1000000; $i++) { $a = $a->next($b); $b = $b->next($a); }
echo get_class($a), ':', get_class($b), "\n";
