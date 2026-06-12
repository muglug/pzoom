<?php
foreach ([1, 2, 3, 4] as $b) {
    if (rand(0, 1)) {
        break;
    }
    $car = "Volvo";
}

echo $car;
