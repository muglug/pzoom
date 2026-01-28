<?php
function foo(?object $a): void {
    if ($a) {
        // do something
    } elseif ($a) {
        // can never get here
    }
}
