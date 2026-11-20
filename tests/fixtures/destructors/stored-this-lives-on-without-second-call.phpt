--TEST--
stored-this-lives-on-without-second-call
--FILE--
<?php
class R { function __destruct() { echo "destructor\n"; $GLOBALS["kept"] = $this; } }
$r = new R;
unset($r);
echo get_class($kept), "\n";
unset($kept);
echo "end\n";
--EXPECT--
destructor
R
end
