--TEST--
The LOG_* constants, openlog() and closelog()
--FILE--
<?php
$names = ['LOG_EMERG', 'LOG_ALERT', 'LOG_CRIT', 'LOG_ERR', 'LOG_WARNING', 'LOG_NOTICE', 'LOG_INFO', 'LOG_DEBUG', 'LOG_KERN', 'LOG_USER', 'LOG_MAIL', 'LOG_DAEMON', 'LOG_AUTH', 'LOG_SYSLOG', 'LOG_LPR', 'LOG_NEWS', 'LOG_UUCP', 'LOG_CRON', 'LOG_AUTHPRIV', 'LOG_LOCAL0', 'LOG_LOCAL1', 'LOG_LOCAL2', 'LOG_LOCAL3', 'LOG_LOCAL4', 'LOG_LOCAL5', 'LOG_LOCAL6', 'LOG_LOCAL7', 'LOG_PID', 'LOG_CONS', 'LOG_ODELAY', 'LOG_NDELAY', 'LOG_NOWAIT', 'LOG_PERROR'];
foreach ($names as $name) echo $name, '=', constant($name), ' ';
echo "\n";
var_dump(openlog('review', LOG_PID | LOG_NDELAY, LOG_USER), closelog(), function_exists('syslog'), function_exists('openlog'), function_exists('closelog'));
try { syslog(); } catch (ArgumentCountError $e) { echo $e->getMessage(), "\n"; }
try { openlog('x'); } catch (ArgumentCountError $e) { echo $e->getMessage(), "\n"; }
--EXPECT--
LOG_EMERG=0 LOG_ALERT=1 LOG_CRIT=2 LOG_ERR=3 LOG_WARNING=4 LOG_NOTICE=5 LOG_INFO=6 LOG_DEBUG=7 LOG_KERN=0 LOG_USER=8 LOG_MAIL=16 LOG_DAEMON=24 LOG_AUTH=32 LOG_SYSLOG=40 LOG_LPR=48 LOG_NEWS=56 LOG_UUCP=64 LOG_CRON=72 LOG_AUTHPRIV=80 LOG_LOCAL0=128 LOG_LOCAL1=136 LOG_LOCAL2=144 LOG_LOCAL3=152 LOG_LOCAL4=160 LOG_LOCAL5=168 LOG_LOCAL6=176 LOG_LOCAL7=184 LOG_PID=1 LOG_CONS=2 LOG_ODELAY=4 LOG_NDELAY=8 LOG_NOWAIT=16 LOG_PERROR=32 
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
syslog() expects exactly 2 arguments, 0 given
openlog() expects exactly 3 arguments, 1 given
