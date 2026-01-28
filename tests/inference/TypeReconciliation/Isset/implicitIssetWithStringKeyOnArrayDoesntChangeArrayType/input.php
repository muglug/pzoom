<?php
class A {}

function run1(array $arguments): void {
    if ($arguments["a"] instanceof A) {}

    if ($arguments["b"]) {
        /** @psalm-suppress MixedArgument */
        echo $arguments["b"];
    }
}