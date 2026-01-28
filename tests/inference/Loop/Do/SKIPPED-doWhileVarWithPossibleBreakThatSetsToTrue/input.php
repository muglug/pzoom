<?php
$a = false;
$b = false;

do {
    $b = true;
    if (rand(0, 1)) {
        $a = true;
        break;
    }
    $a = true;
}
while (rand(0,1));
