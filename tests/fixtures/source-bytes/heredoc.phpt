--TEST--
Heredoc and nowdoc preserve binary source and distinguish escapes
--FILE--
<?php
$v = 'X';
echo <<<DOC
  [ÿ$v]
  DOC;
echo "
";
echo <<<'DOC'
  [ÿ$v]
  DOC;
echo "
";
--EXPECT--
[ÿX]
[ÿ$v]
