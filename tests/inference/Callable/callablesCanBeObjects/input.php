<?php
function foo(callable $c) : void {
    if (is_object($c)) {
        $c();
    }
}
