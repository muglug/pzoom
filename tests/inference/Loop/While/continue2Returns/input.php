<?php
function foo(): array {
    while (rand(0, 1)) {
        while (rand(0, 1)) {
            if (rand(0, 1)) {
                continue 2;
            }

            return [];
        }
    }

    return [];
}
