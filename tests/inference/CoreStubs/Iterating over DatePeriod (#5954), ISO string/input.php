<?php

$period = new DatePeriod("R4/2012-07-01T00:00:00Z/P7D");
$dt = null;
foreach ($period as $dt) {
    echo $dt->format("Y-m-d");
}
