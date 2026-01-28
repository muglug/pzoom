<?php
class A {}

/**
 * @psalm-suppress MixedArgument
 */
function run1(array $arguments): void {
    if (rand(0, 1)) {
        $arguments["c"] = new A();
    }

    if ($arguments["b"]) {
        echo $arguments["b"];
    }
}
