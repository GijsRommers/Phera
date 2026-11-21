--TEST--
PCRE count-only matches use current INI settings and preserve errors
--FILE--
<?php
function match_without_output($pattern, $subject) {
    var_dump(preg_match($pattern, $subject), preg_last_error());
}
function count_without_output($pattern, $subject) {
    var_dump(preg_match_all($pattern, $subject, flags: PREG_SET_ORDER), preg_last_error());
}
match_without_output('/(?:|a)/', 'a');
count_without_output('/(?:|a)/', 'a');
match_without_output('/./u', "\xff");
count_without_output('/([a-z]+)(\d+)/', 'a12 bb45 z');
var_dump(preg_match('/(?<letters>[a-z]+)(\d+)(*MARK:word)/', 'abc12', $matches), $matches);
var_dump(preg_match('/./u', 'é', offset: 1), preg_last_error());
$pattern = '/^(a+)+$/';
$subject = 'aaaaaaaa!';
ini_set('pcre.jit', '0');
match_without_output($pattern, $subject);
ini_set('pcre.backtrack_limit', '1');
match_without_output($pattern, $subject);
ini_restore('pcre.backtrack_limit');
match_without_output($pattern, $subject);
ini_set('pcre.recursion_limit', '1');
match_without_output($pattern, $subject);
ini_restore('pcre.recursion_limit');
match_without_output($pattern, $subject);
ini_restore('pcre.jit');
--EXPECT--
int(1)
int(0)
int(3)
int(0)
bool(false)
int(4)
int(2)
int(0)
int(1)
array(5) {
  [0]=>
  string(5) "abc12"
  ["letters"]=>
  string(3) "abc"
  [1]=>
  string(3) "abc"
  [2]=>
  string(2) "12"
  ["MARK"]=>
  string(4) "word"
}
bool(false)
int(5)
int(0)
int(0)
bool(false)
int(2)
int(0)
int(0)
bool(false)
int(3)
int(0)
int(0)
