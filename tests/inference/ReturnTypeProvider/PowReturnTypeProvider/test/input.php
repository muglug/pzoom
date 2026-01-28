<?php
function getInt(): int {
    return 1;
}
function getFloat(): float {
    return 1.0;
}
$int = getInt();
$float = getFloat();

$a = pow($int, $int);
$b = pow($int, $float);
$c = pow($float, $int);
$d = pow(1000, 1000);
$e = pow(0, 1000);
$f = pow(1000, 0);
            
