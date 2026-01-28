<?php
$a = false;
switch (rand(0, 4)) {
    case 0:
        $b = 1;
        if (rand(0, 1)) {
            $a = false;
            break;
        }

    default:
        $a = true;
}
