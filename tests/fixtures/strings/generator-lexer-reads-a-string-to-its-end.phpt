--TEST--
A lexer loop over isset($content[$offset]) stops at the end of the string
--FILE--
<?php
$patterns = ['[\r\n]{1,1000}', '[^\S\r\n]{1,1000}', '\\\\', '\'', '"', '\\#', '\\$', '([^(\s\\\\\'"\\#\\$)]|\\(|\\)){1,1000}'];
function lex(string $content, array $patterns) {
    static $regex;
    if ($regex === null) { $regex = '((' . implode(')|(', $patterns) . '))A'; }
    $offset = 0;
    while (isset($content[$offset])) {
        if (preg_match($regex, $content, $matches, 0, $offset) !== 1) { throw new Error(sprintf('unexpected [%s] at %d', $content[$offset], $offset)); }
        $offset += strlen($matches[0]);
        yield $matches[0];
    }
}
$env = "APP_NAME=Laravel\nAPP_KEY=base64:abc+/=\nAPP_URL=\"http://localhost\" # comment\nX='it'\nY=\${APP_NAME}\\n\n";
try { foreach (lex($env, $patterns) as $t) { echo json_encode($t), ' '; } } catch (Error $e) { echo "\nERROR ", $e->getMessage(); }
echo "\n";
var_dump(preg_match('((a)|(b))A', 'xab', $m, 0, 1), $m, preg_match('(b)A', 'xab', $m, 0, 1), preg_match('/\G[^\S\r\n]{1,5}/', "a  b", $m, 0, 1), $m);
$s = "abc"; $o = 3; var_dump(isset($s[$o]), isset($s[2]), isset($s[-1]), isset($s[-4]));
--EXPECT--
"APP_NAME=Laravel" "\n" "APP_KEY=base64:abc+\/=" "\n" "APP_URL=" "\"" "http:\/\/localhost" "\"" " " "#" " " "comment" "\n" "X=" "'" "it" "'" "\n" "Y=" "$" "{APP_NAME}" "\\" "n" "\n" 
int(1)
array(2) {
  [0]=>
  string(1) "a"
  [1]=>
  string(1) "a"
}
int(0)
int(1)
array(1) {
  [0]=>
  string(2) "  "
}
bool(false)
bool(true)
bool(true)
bool(false)
