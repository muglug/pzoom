<?php
function breakUpPathIntoParts(): void {
    $b = false;

    while (rand(0, 1)) {
        if ($b) {
            echo "hello";

            continue;
        }

        $b = true;
    }
}
