<?php
function foo(string $s) : void {
    if (enum_exists($s)) {
        new ReflectionClass($s);
    }
}
