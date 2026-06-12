<?php
function foo(bool $x): never
{
    while (true) {
        if ($x) {
            break;
        }
    }
}
