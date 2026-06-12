<?php
function foo(callable $c) : void {
    if (is_array($c)) {
        echo $c[1];
    }
}
