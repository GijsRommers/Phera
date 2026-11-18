<?php
$date = new DateTimeImmutable("2024-02-29 12:00:00 UTC");
$interval = new DateInterval("P1Y");
$added = $date->add($interval);
$subtracted = $date->sub($interval);
echo $added->format('Y-m-d\TH:i:s.uP e I'), "\n";
echo $subtracted->format('Y-m-d\TH:i:s.uP e I'), "\n";
echo $date->diff($added)->format('%R%Y-%M-%D %H:%I:%S.%F %a %r %y %m %d %h %i %s %f %% %q %'), "\n";
echo $added->diff($date)->format('%R%Y-%M-%D %H:%I:%S.%F %a %r %y %m %d %h %i %s %f %% %q %'), "\n";
