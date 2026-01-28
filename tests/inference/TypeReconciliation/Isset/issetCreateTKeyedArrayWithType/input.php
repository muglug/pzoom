<?php
function foo(array $options): void {
    if (isset($options["a"])) {
        $options["b"] = "hello";
    }

    if (\is_array($options["b"])) {}
}