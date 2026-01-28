<?php
$x = rand(0, 10) > 5 ? new stdClass : null;
if ($x instanceof stdClass && $x) {}
