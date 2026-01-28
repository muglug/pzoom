<?php
$date = new DateTime("now", new DateTimeZone("Pacific/Nauru"));
echo $date->format("Y-m-d H:i:sP") . "\n";
