<?php
for ($i = 0; $i < 4; $i++) {
    while (rand(0,10) === 5) {
        $array[] = "hello";
    }
}

echo $array;
