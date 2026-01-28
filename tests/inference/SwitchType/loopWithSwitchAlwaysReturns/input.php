<?php
function b(): int {
    foreach([1,2] as $i) {
        continue;
    }

    switch (random_int(1, 10)) {
        case 1:
            return 1;
        default:
            return 2;
    }
}
