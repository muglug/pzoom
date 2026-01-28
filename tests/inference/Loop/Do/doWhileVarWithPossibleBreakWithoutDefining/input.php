<?php
do {
    if (rand(0, 1)) {
        break;
    }
    $worked = true;
}
while (rand(0,1));

echo $worked;
