<?php
/** @return void */
function foo() {
    static $bar = null;

    if ($bar !== null) {
        // do something
    }

    $bar = 5;
}