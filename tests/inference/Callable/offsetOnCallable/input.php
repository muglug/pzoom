<?php
function c(callable $c) : void {
    if (is_array($c)) {
        new ReflectionClass($c[0]);
    }
}
