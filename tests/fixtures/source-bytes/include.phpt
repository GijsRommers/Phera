--TEST--
Include and require read binary files through the same source loader
--FILE--
<?php
include __DIR__ . '/binary-helper.inc';
require __DIR__ . '/binary-helper.inc';
--EXPECT--
1:2:ÿ
1:2:ÿ
