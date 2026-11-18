--TEST--
Single-quoted and double-quoted PHP strings preserve binary source
--FILE--
<?php
echo '[', 'ÿ', "] [", "ÿ", "] [", "ÿ\n", "]
";
--EXPECT--
[ÿ] [ÿ] [ÿ
]
