<?php
$x = false;
$y = false;

foreach ([1, 2, 3] as $v)  {
    switch($v) {
        case 3:
            $y = true;
            break;
        case 2:
            $x = true;
            break;
        default:
            break;
    }
}
