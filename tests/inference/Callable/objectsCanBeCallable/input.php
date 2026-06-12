<?php
function foo(object $c) : void {
    if (is_callable($c)) {
        $c();
    }
}
