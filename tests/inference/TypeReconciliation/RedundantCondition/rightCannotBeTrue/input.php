<?php
/** @param false $a */
function foo(bool $a): void {
    if (rand(0, 1) || $a) {
        echo "a or b";
    }
}
