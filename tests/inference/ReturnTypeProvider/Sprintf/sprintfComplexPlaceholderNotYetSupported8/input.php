<?php
$precision = 1;
$width = 10;
$flt = 1.234;
$val = sprintf("%3\$*2\$.*1\$f", $precision, $width, $flt);
