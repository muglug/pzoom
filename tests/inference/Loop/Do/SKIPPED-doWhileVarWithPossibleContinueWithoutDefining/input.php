<?php
do {
    if (rand(0, 1)) {
        continue;
    }
    $worked = true;
}
while (rand(0,1));

echo $worked;
