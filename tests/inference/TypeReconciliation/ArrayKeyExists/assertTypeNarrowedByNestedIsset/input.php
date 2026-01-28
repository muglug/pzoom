<?php
/**
 * @psalm-suppress MixedMethodCall
 * @psalm-suppress MixedArgument
 */
function foo(array $array = []): void {
    if (array_key_exists("a", $array)) {
        echo $array["a"];
    }

    if (array_key_exists("b", $array)) {
        echo $array["b"]->format("Y-m-d");
    }
}