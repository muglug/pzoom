<?php
switch (rand(0, 4)) {
    case 0:
        $b = 2;
        if (rand(0, 1)) {
            $a = false;
            break;
        }

    default:
        $a = true;
        $b = 1;
}
