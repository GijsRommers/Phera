<?php
$date = new DateTimeImmutable("+25000-02-29 01:02:03.456789 UTC");
$interval = new DateInterval("P400Y");
$added = $date->add($interval);
$subtracted = $date->sub($interval);
echo $added->format('Y-m-d\TH:i:s.uP e I'), "\n";
echo $subtracted->format('Y-m-d\TH:i:s.uP e I'), "\n";
echo $date->diff($added)->format('%R%Y-%M-%D %H:%I:%S.%F %a %r %y %m %d %h %i %s %f %% %q %'), "\n";
echo $added->diff($date)->format('%R%Y-%M-%D %H:%I:%S.%F %a %r %y %m %d %h %i %s %f %% %q %'), "\n";
