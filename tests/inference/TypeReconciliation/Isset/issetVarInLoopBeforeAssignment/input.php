<?php
function foo() : void {
    while (rand(0, 1)) {
        if (!isset($foo)) {
            $foo = 1;
        }
    }
}