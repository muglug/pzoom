<?php
do {
    if (rand(0, 1)) {
        $worked = true;
        break;
    }
    $worked = true;
}
while (rand(0,100) === 10);
