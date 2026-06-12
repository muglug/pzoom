<?php
function foo(): void {}

function callMeMaybe(string $method): void {
    if (is_callable($method)) {
        $method();
    }
}

callMeMaybe("foo");
