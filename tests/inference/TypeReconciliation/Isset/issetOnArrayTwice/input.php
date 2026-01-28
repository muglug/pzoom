<?php
function foo(array $options): void {
    if (!isset($options["a"])) {
        $options["a"] = "hello";
    }

    if (!isset($options["b"])) {
        $options["b"] = 1;
    }

    if ($options["b"] === 2) {}
}