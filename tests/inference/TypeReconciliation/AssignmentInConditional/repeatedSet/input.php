<?php
function foo(): void {
    if ($a = rand(0, 1) ? "1" : null) {
        return;
    }

    if (rand(0, 1)) {
        $a = rand(0, 1) ? "hello" : null;

        if ($a) {

        }
    }
}